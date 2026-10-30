//! Loading presentation over the same SDL clock, renderer and framebuffer.
use super::{
    display::{copy_framebuffer, Output},
    session_host::AppRender,
    AppClock,
};
use crate::{
    draw_queue::DrawInvocation,
    frontend_scene::FrontendSceneHost,
    frontend_scene::SceneCommand,
    loading_draw::{draw_loading_tick_40e6b0, LoadingDrawHost, LoadingDrawRequest},
    loading_integration::{LoadingDrawInputs, LoadingPresentationHost},
    loading_model::{
        draw_loading_model_40e7cb, LoadingModelHost, LoadingModelProjection, LoadingModelRequest,
        LoadingModelTemplate,
    },
    loading_runtime::LoadingProcessState,
    menu_basis::MenuBasisMath,
    menu_text::{MenuGlyphQuad, MenuTextHost},
    pixel_backend::PixelBackend,
    presentation::OutputPresentation,
    render_resources::RenderResources,
    scene_resources::ResidentSceneHost,
    scene_sprite::DirectSpriteQuad,
    session_platform::software::present_linked,
    software_menu::glyph_command,
    software_scene::{CurrentResourceDraw, ResourceBatchPolicy, ResourceLoan},
    ResidentRuntime,
};
use sdl2::video::Window;
use std::cell::Cell;

#[derive(Debug, Default)]
pub struct LoadingPresentationStats {
    pub callbacks: usize,
    pub presentations: usize,
    pub contained_errors: usize,
    pub model_background_draws: usize,
}

pub(super) struct SdlLoadingPresentation<'a, 'b, 't, 'f, 'r> {
    pub clock: &'a mut AppClock,
    pub output: &'a mut Output,
    pub render: Option<&'a mut AppRender<'b, 't, 'f, 'r>>,
    pub stats: &'a mut LoadingPresentationStats,
    pub basis: &'a MenuBasisMath,
}

impl LoadingPresentationHost for SdlLoadingPresentation<'_, '_, '_, '_, '_> {
    fn sample_clock(&mut self) -> u64 {
        self.clock.sample_in_frame()
    }
    fn renderer_available(&mut self) -> Result<bool, String> {
        self.stats.callbacks += 1;
        Ok(self.render.is_some())
    }
    fn tick_loading(&mut self, request: LoadingDrawInputs<'_>) -> Result<(), String> {
        let render = self
            .render
            .as_deref_mut()
            .ok_or("loading renderer disappeared")?;
        let resources = RenderResources::new(request.residency);
        // The CPU scene's retained projection center is the current glyph
        // context metadata, as in the existing shared text submission owner.
        // It is neither a zero default nor added again to screen coordinates.
        let origin = render.scene.primitive_state.projection.center;
        let window = self.output.window();
        let presentation = render.presentation;
        let stats = &mut *self.stats;
        // Actual session+648 lookup was performed inside the callback guard.
        // Native template context+0 aliases this captured renderer; model
        // dimensions and projection still have their own reached publications.
        let _incoming_context = request.context;
        render
            .scene
            .with_current_resources(
                CurrentResourceDraw {
                    resources,
                    batch_policy:
                        ResourceBatchPolicy::CompletedResourceBatchRetirementApproximation,
                },
                |loan| {
                    let mut host = LoadingDraw {
                        loan,
                        resources,
                        window,
                        presentation,
                        current_attract: request.current_attract,
                        current_mode: request.current_mode,
                        stats,
                        model_template: render.loading_model_template,
                        focal: render.template_projection.focal,
                        basis: self.basis,
                    };
                    draw_loading_tick_40e6b0(
                        LoadingDrawRequest {
                            tick: request.tick,
                            context_origin_pixels: origin,
                        },
                        &resources,
                        &mut host,
                    )
                    .map_err(|error| error.to_string())
                },
            )
            .map_err(|error| format!("loading resource loan: {error:?}"))
    }
    fn present_loading(&mut self, residency: &ResidentRuntime) -> Result<(), String> {
        // 0040ED50: fresh renderer acquisition and fresh current H7/32. Loading has
        // neither the title nor gameplay forced-black presentation policy.
        let render = self
            .render
            .as_deref_mut()
            .ok_or("loading presentation renderer disappeared")?;
        let presentation = render.presentation;
        let output = &mut *self.output;
        let texture = render.texture.as_mut();
        let stats = &mut *self.stats;
        present_linked(
            &mut *render.scene,
            RenderResources::new(residency),
            crate::frontend_display::PresentationClear::Standard,
            false,
            |frame| {
                copy_framebuffer(output, texture, frame, presentation.scaling)?;
                super::display::present_observed(output, render.capture.as_ref(), "loading", None);
                if let Some(capture) = &render.capture {
                    capture.check().map_err(|error| error.to_string())?;
                }
                stats.presentations += 1;
                Ok::<(), String>(())
            },
        )
        .map_err(|error| error.to_string())
    }
    fn dispose_loading_error(&mut self, error: String) {
        self.stats.contained_errors += 1;
        eprintln!("Infestation: loading callback contained error: {error}");
    }
}

struct LoadingDraw<'a, 'scope, 'epoch, 'w> {
    loan: &'a mut ResourceLoan<'scope, 'epoch>,
    resources: RenderResources<'scope>,
    window: &'w Window,
    presentation: OutputPresentation,
    current_attract: &'a bool,
    current_mode: &'a Cell<u8>,
    stats: &'a mut LoadingPresentationStats,
    model_template: &'a LoadingModelTemplate,
    focal: [i32; 2],
    basis: &'a MenuBasisMath,
}

impl LoadingDraw<'_, '_, '_, '_> {
    fn surface_dimensions(&self) -> [u32; 2] {
        if self.presentation.output == self.loan.dimensions() {
            self.window.size().into()
        } else {
            self.loan.dimensions()
        }
    }
    fn validate_surface(&self) -> Result<(), String> {
        let actual: [u32; 2] = self.window.size().into();
        let expected = self.presentation.output;
        if actual == expected {
            Ok(())
        } else {
            Err(format!(
                "loading surface dimensions {actual:?} differ from {expected:?}"
            ))
        }
    }
}

impl MenuTextHost for LoadingDraw<'_, '_, '_, '_> {
    type Error = String;
    fn submit_glyph(&mut self, glyph: MenuGlyphQuad) -> Result<(), String> {
        let mut command =
            glyph_command(self.resources, glyph).map_err(|error| error.to_string())?;
        let scene = self
            .loan
            .scene_after_reset()
            .map_err(|error| error.to_string())?;
        scene
            .pixels
            .draw(
                DrawInvocation {
                    command_offset: 0,
                    receiver_offset: 0,
                },
                &mut command,
            )
            .map_err(|error| error.to_string())
    }
}

impl LoadingDrawHost for LoadingDraw<'_, '_, '_, '_> {
    fn acquire_renderer(&mut self) -> Result<(), String> {
        let _current = self.loan.renderer_id();
        Ok(())
    }
    fn surface_width(&mut self) -> Result<i32, String> {
        Ok(self.surface_dimensions()[0] as i32)
    }
    fn surface_height(&mut self) -> Result<i32, String> {
        Ok(self.surface_dimensions()[1] as i32)
    }
    fn acquire_surface(&mut self) -> Result<(), String> {
        // CPU pixels have no DirectDraw lock. Only the genuine live surface
        // validation is mapped; native lock/atlas allocation parity is open.
        self.validate_surface()
    }
    fn reset_batch_40e6fd(&mut self) -> Result<(), String> {
        self.loan
            .reset_batch_40e6fd()
            .map_err(|error| error.to_string())
    }
    fn reset_surface_40e70a(&mut self) -> Result<(), String> {
        self.loan
            .scene_after_reset()
            .map_err(|error| error.to_string())?
            .reset_renderer();
        Ok(())
    }
    fn current_attract_mode(&mut self) -> u8 {
        u8::from(*self.current_attract)
    }
    fn current_loading_mode(&mut self) -> u8 {
        self.current_mode.get()
    }
    fn draw_model_background(
        &mut self,
        state: &LoadingProcessState,
        rng: &mut u32,
    ) -> Result<(), String> {
        self.loan
            .scene_after_reset()
            .map_err(|error| error.to_string())?
            .set_process_rng(Some(*rng));
        let result = draw_loading_model_40e7cb(
            LoadingModelRequest {
                state,
                template: self.model_template,
                focal: self.focal,
                math: self.basis,
            },
            self,
        );
        // Commit genuine jitter/RNG prefix even when a later command fails.
        *rng = self
            .loan
            .scene_after_reset()
            .map_err(|error| error.to_string())?
            .process_rng()
            .ok_or("loading model lost its actual process RNG loan")?;
        result?;
        self.stats.model_background_draws += 1;
        Ok(())
    }
    fn finish_background_495770(&mut self) -> Result<(), String> {
        self.validate_surface()
    }
    fn flush_batch_4a4460(&mut self) -> Result<(), String> {
        let scene = self
            .loan
            .scene_after_reset()
            .map_err(|error| error.to_string())?;
        ResidentSceneHost {
            resources: self.resources,
            backend: scene,
        }
        .submit(SceneCommand::FlushBatch)
        .map_err(|error| format!("loading flush: {error:?}"))
    }
    fn submit_loading_quad(&mut self, quad: DirectSpriteQuad) -> Result<(), String> {
        self.loan
            .scene_after_reset()
            .map_err(|error| error.to_string())?
            .draw_sprite_direct(quad)
            .map_err(|error| error.to_string())
    }
}

impl LoadingModelHost for LoadingDraw<'_, '_, '_, '_> {
    type Error = String;
    fn surface_width(&mut self) -> Result<i32, String> {
        LoadingDrawHost::surface_width(self)
    }
    fn surface_height(&mut self) -> Result<i32, String> {
        LoadingDrawHost::surface_height(self)
    }
    fn submit_loading_scene(&mut self, command: SceneCommand) -> Result<(), String> {
        let scene = self
            .loan
            .scene_after_reset()
            .map_err(|error| error.to_string())?;
        ResidentSceneHost {
            resources: self.resources,
            backend: scene,
        }
        .submit(command)
        .map_err(|error| format!("loading model: {error:?}"))
    }
    fn publish_loading_projection(
        &mut self,
        projection: LoadingModelProjection,
    ) -> Result<(), String> {
        let scene = self
            .loan
            .scene_after_reset()
            .map_err(|error| error.to_string())?;
        scene
            .publish_model_window(crate::process_model_graphics::ModelProjectionWindow {
                focal: projection.focal,
                viewport: projection.viewport,
                center: projection.center,
            })
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}
