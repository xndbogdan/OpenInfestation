//! Scoped current-resource draws over the actual retained renderer and frame.
//!
//! Native loading Tick clears its batch at0040E6FD, after renderer/dimension/
//! preparation reads. No deferred resource-backed command may escape this loan.
//! CompletedResourceBatchRetirementApproximation permits an explicitly selected
//! successful, fully replayed batch to retire at scope exit. Native flush retains
//! its packets; later replay and retained-packet custody are not admitted here.
//! Pending/error prefixes remain typed containment boundaries. Old epoch caches
//! stay untouched. Fresh direct uploads use temporary CPU cache custody, a
//! performance policy, not native atlas allocation or texture identity parity.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceBatchPolicy {
    DirectOnly,
    /// Retire short-lived payloads only after the actual full replay and a
    /// successful callback. This does not claim native retained arena custody.
    CompletedResourceBatchRetirementApproximation,
}

pub struct CurrentResourceDraw<'a> {
    pub resources: RenderResources<'a>,
    pub batch_policy: ResourceBatchPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ResourceLoanPhaseError {
    #[error("current resource drawing requires the reached0040E6FD batch reset")]
    BatchNotReset,
    #[error("this current resource loan already consumed0040E6FD")]
    BatchAlreadyReset,
}

#[derive(Debug, Error)]
pub enum ResourceLoanError<E> {
    #[error(transparent)]
    Backend(#[from] SoftwareSceneError),
    #[error("current resource callback failed")]
    Callback(E),
    #[error(
        "current resource loan retained {commands} deferred commands outside its admitted domain"
    )]
    DeferredCommands {
        commands: usize,
        /// Preserve a reached callback error when the unsupported queue prefix
        /// also prevents returning short-lived resource custody.
        callback_error: Option<E>,
    },
}

/// The actual scene's short frame/scalar loan plus its old deferred arena.
/// Only immutable renderer/dimension reads are exposed before native reset.
pub struct ResourceLoan<'scope, 'epoch> {
    scene: SoftwareScene<'scope, 'scope>,
    old_queue: &'scope mut DrawQueue<PrimitiveCommand<'epoch>>,
    reset_reached: bool,
}

impl<'scope, 'epoch> ResourceLoan<'scope, 'epoch> {
    pub fn renderer_id(&self) -> SoftwareRendererId {
        self.scene.id
    }

    pub fn dimensions(&self) -> [u32; 2] {
        [
            self.scene.pixels.frame.width(),
            self.scene.pixels.frame.height(),
        ]
    }

    /// Actual 004A4400 call at0040E6FD. Neither construction nor a failed
    /// earlier callback touches the old batch. No arena replacement occurs.
    pub fn reset_batch_40e6fd(&mut self) -> Result<(), ResourceLoanPhaseError> {
        if self.reset_reached {
            return Err(ResourceLoanPhaseError::BatchAlreadyReset);
        }
        self.old_queue.reset();
        self.scene.queue.reset();
        self.scene.stopped = false;
        self.scene.stop_cause = None;
        self.scene.aborted_presentation = false;
        self.scene.stats.begins += 1;
        self.reset_reached = true;
        Ok(())
    }

    pub fn scene_after_reset(
        &mut self,
    ) -> Result<&mut SoftwareScene<'scope, 'scope>, ResourceLoanPhaseError> {
        if !self.reset_reached {
            return Err(ResourceLoanPhaseError::BatchNotReset);
        }
        Ok(&mut self.scene)
    }
}

impl<'frame, 'epoch> SoftwareScene<'frame, 'epoch> {
    /// Borrow current resources without replacing this renderer or framebuffer.
    /// Result errors retain every reached scalar/pixel prefix. The caller owns
    /// callback error containment; this method never retries or presents.
    pub fn with_current_resources<'scope, T, E>(
        &'scope mut self,
        request: CurrentResourceDraw<'scope>,
        callback: impl FnOnce(&mut ResourceLoan<'scope, 'epoch>) -> Result<T, E>,
    ) -> Result<T, ResourceLoanError<E>> {
        // Mechanical temporary storage has no native reset side effect. Its
        // capacity is the actual retained arena capacity, not C8000/defaults.
        let queue = DrawQueue::with_native_memory(
            self.queue.state().capacity,
            flash::record_primitive_memory,
        )
        .map_err(SoftwareSceneError::from)?;
        let mut pixels = SoftwarePixelBackend::new(
            &mut *self.pixels.frame,
            self.pixels.renderer,
            self.pixels.pipeline,
            self.pixels.texture_state,
        );
        pixels.stats = copy_pixel_stats(&self.pixels.stats);
        let mut loan = ResourceLoan {
            scene: SoftwareScene {
                scratch: self.scratch.clone(),
                graphics: self.graphics.clone(),
                id: self.id,
                resources: request.resources,
                tables: self.tables,
                execution_limits: self.execution_limits,
                color_layout: self.color_layout,
                environment: self.environment,
                primitive_state: self.primitive_state,
                queue,
                pixels,
                stats: self.stats,
                texture_identities: TextureIdentities::default(),
                terrain: self.terrain.clone(),
                surface_callbacks: self.surface_callbacks,
                furniture_omissions: self.furniture_omissions.clone(),
                water_omissions: self.water_omissions.clone(),
                process_rng: self.process_rng,
                mirror_copy: self.mirror_copy,
                hovercraft_fans: self.hovercraft_fans,
                furniture_root_registers: Default::default(),
                guided_mounts: self.guided_mounts,
                stopped: self.stopped,
                stop_cause: self.stop_cause.take(),
                aborted_presentation: self.aborted_presentation,
                canvas_viewport: self.canvas_viewport,
                frame_viewport: self.frame_viewport,
                // A feed borrows the world's own resources; loans draw none.
                scene_feed: None,
                raster_motion: None,
                exact_corners: false,
                ironbow: false,
                draw_tint: None,
                water_fog: self.water_fog,
                raster_sky: None,
                raster_metals: Default::default(),
                game_draw: None,
                game_queue: None,
            },
            old_queue: &mut self.queue,
            reset_reached: false,
        };
        let result = callback(&mut loan);
        let commands = loan.scene.queue.state().command_count;
        let retire_completed = commands != 0
            && request.batch_policy
                == ResourceBatchPolicy::CompletedResourceBatchRetirementApproximation
            && result.is_ok()
            && loan.reset_reached
            && !loan.scene.stopped
            && loan.scene.queue.is_fully_replayed();
        // Exhaustive owned-field extraction keeps future state additions from
        // silently disappearing at the loan boundary. Resource-backed caches
        // belong to the short view and are deliberately not copied back.
        let SoftwareScene {
            scratch: _,
            graphics: _,
            id: _,
            resources: _,
            tables: _,
            execution_limits,
            color_layout,
            environment,
            primitive_state,
            queue: _,
            pixels,
            stats,
            texture_identities: _,
            terrain,
            surface_callbacks,
            furniture_omissions,
            water_omissions,
            process_rng,
            mirror_copy,
            hovercraft_fans,
            furniture_root_registers: _,
            guided_mounts,
            stopped,
            stop_cause,
            aborted_presentation,
            canvas_viewport,
            frame_viewport,
            scene_feed: _,
            raster_motion: _,
            exact_corners: _,
            ironbow: _,
            draw_tint: _,
            water_fog: _,
            raster_sky: _,
            raster_metals: _,
            game_draw: _,
            game_queue: _,
        } = loan.scene;
        let SoftwarePixelBackend {
            frame: _,
            stats: pixel_stats,
            renderer,
            pipeline,
            texture_state,
            textures: _,
            ..
        } = pixels;
        self.execution_limits = execution_limits;
        self.color_layout = color_layout;
        self.environment = environment;
        self.primitive_state = primitive_state;
        self.stats = stats;
        self.terrain = terrain;
        self.surface_callbacks = surface_callbacks;
        self.furniture_omissions = furniture_omissions;
        self.water_omissions = water_omissions;
        self.process_rng = process_rng;
        self.mirror_copy = mirror_copy;
        self.hovercraft_fans = hovercraft_fans;
        self.guided_mounts = guided_mounts;
        self.stopped = stopped;
        self.stop_cause = stop_cause;
        self.aborted_presentation = aborted_presentation;
        self.canvas_viewport = canvas_viewport;
        self.frame_viewport = frame_viewport;
        self.pixels.stats = pixel_stats;
        self.pixels.renderer = renderer;
        self.pixels.pipeline = pipeline;
        self.pixels.texture_state = texture_state;
        if retire_completed {
            self.stats.retired_resource_batches += 1;
        } else if commands != 0 {
            // The caller exceeded the selected resource lifetime domain.
            // Do not silently continue a batch whose deferred custody cannot
            // survive; a later genuine native reset can release containment.
            if !self.stopped {
                self.stop_cause = Some(StopCause {
                    text: format!("{commands} deferred command(s) outlived their resource loan"),
                    overflow: false,
                });
            }
            self.stopped = true;
            return Err(ResourceLoanError::DeferredCommands {
                commands,
                callback_error: result.err(),
            });
        }
        result.map_err(ResourceLoanError::Callback)
    }
}

fn copy_pixel_stats(stats: &SoftwarePixelStats) -> SoftwarePixelStats {
    let SoftwarePixelStats {
        triangles,
        solid_quads,
        solid_triangles,
        textured_triangles,
        shared_shade_far_triangles,
        textured_quads,
        ordinary_far_quads,
        textured_uv_triangles,
        textured_uv_quads,
        shared_shade_uv_triangles,
        shared_shade_uv_quads,
        effect_quads,
        d2_triangles,
        d6_quads,
        textured_shade_uv_quads,
        colored_quads,
        da_triangles,
        flat_shade_quads,
        skipped,
        fragments,
        unsupported,
        traced,
    } = *stats;
    SoftwarePixelStats {
        triangles,
        solid_quads,
        solid_triangles,
        textured_triangles,
        shared_shade_far_triangles,
        textured_quads,
        ordinary_far_quads,
        textured_uv_triangles,
        textured_uv_quads,
        shared_shade_uv_triangles,
        shared_shade_uv_quads,
        effect_quads,
        d2_triangles,
        d6_quads,
        textured_shade_uv_quads,
        colored_quads,
        da_triangles,
        flat_shade_quads,
        skipped,
        fragments,
        unsupported,
        traced,
    }
}

#[cfg(test)]
mod tests;
