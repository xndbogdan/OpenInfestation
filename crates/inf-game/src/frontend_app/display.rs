//! Transactional SDL display replacement at the native 00437DF0 boundary.
//!
//! The active renderer keeps borrowing its old registration table until the
//! input pass ends. A successful reset already owns validated replacement
//! resources, pixels and texture, and changes the actual SDL surface. Later
//! input effects resolve the new resources immediately; drawing waits until
//! the old renderer is retired and the prepared replacement is installed.

use sdl2::{
    pixels::PixelFormatEnum,
    render::{Canvas, Texture, TextureCreator},
    video::{
        DisplayMode as SdlMode, FullscreenType, GLProfile, SwapInterval, Window, WindowContext,
        WindowPos,
    },
    VideoSubsystem,
};

use crate::{
    display_selection::{DisplayMode, DisplaySelection, MonitorModes},
    frame_device::Device,
    frontend_app::FrontendAppStartup,
    frontend_display::{frontend_display_dimensions, frontend_startup},
    frontend_runtime::{FrontendDisplayHost, FrontendRuntimeError},
    frontend_video::{apply_video_pipeline, VideoDevice, VideoResetRequest},
    gl_device::GlDevice,
    model_execution::ModelEnvironment,
    model_primitives::{PrimitiveState, PrimitiveTables},
    model_process::ModelProcessOwners,
    presentation::{
        ClassicScaling, NativeFrame, OutputPresentation, RendererBackend, RendererChoice,
    },
    render_resources::RenderResources,
    software_frame::SoftwareSurface,
    software_frame::{SoftwareFrame, SoftwarePixelStats},
    software_scene::{SoftwareScene, SoftwareSceneRequest, SoftwareSceneStats},
    vk_device::VkDevice,
    ResidencyRequest, ResidentRuntime,
};

pub(super) struct PreparedDisplay<'t> {
    pub runtime: ResidentRuntime,
    pub frame: SoftwareFrame,
    /// The SDL presentation texture; an OpenGL frame presents itself.
    pub texture: Option<Texture<'t>>,
    pub startup: FrontendAppStartup,
    pub dimensions: [u32; 2],
    /// The window's physical size.
    pub output: [u32; 2],
}

/// Process DPI awareness for SDL's video start: sizes, modes and windows are
/// then physical pixels, as on a monitor without scaling.
pub(super) fn declare_dpi_awareness() {
    sdl2::hint::set("SDL_WINDOWS_DPI_AWARENESS", "permonitorv2");
}

impl OutputPresentation {
    /// The surface native frames draw on: the window itself at the native
    /// size, otherwise the native frame the output enlarges.
    pub(super) fn surface<'w>(self, window: &'w Window, native: [u32; 2]) -> SoftwareSurface<'w> {
        if self.output == native {
            SoftwareSurface::Window(window)
        } else {
            SoftwareSurface::ClassicFramebuffer {
                window,
                output_dimensions: self.output,
            }
        }
    }
}

/// The window an output opens: for OpenGL, for Vulkan, or for SDL's
/// software canvas.
#[derive(Clone, Copy)]
enum WindowKind {
    OpenGl,
    Vulkan,
    Canvas,
}

/// The game window and its renderer. Retail always draws through Direct3D,
/// which falls back to its own software emulation; the port prefers OpenGL
/// and falls back to the CPU raster, which is also the deterministic choice.
pub(super) enum Output {
    /// SDL software canvas showing CPU frames.
    Canvas(Canvas<Window>),
    /// A hardware device's window; frames draw on its device and present
    /// themselves. The device drops first, while its window is still current.
    Hardware { device: Device, window: Window },
}

impl Output {
    pub fn open(
        video: &VideoSubsystem,
        choice: RendererChoice,
        size: [u32; 2],
        hidden: bool,
    ) -> Result<Self, String> {
        let window = |kind: WindowKind| {
            // Retail's entry (00437BD0, whose start and end log lines go to a
            // RET) passes the title "Infestation" (00533434) and icon 0x65 to
            // its platform loop. The window (004F4C10, class "Frame") opens
            // untitled, and 004958B0 titles it once a screen is set up; this
            // one has the title from the start.
            let mut builder = video.window("Infestation", size[0], size[1]);
            builder.position_centered();
            if hidden {
                builder.hidden();
            }
            match kind {
                WindowKind::OpenGl => {
                    builder.opengl();
                }
                WindowKind::Vulkan => {
                    builder.vulkan();
                }
                WindowKind::Canvas => {}
            }
            builder.build().map_err(|error| error.to_string())
        };
        if choice == RendererChoice::Vulkan {
            let opened = window(WindowKind::Vulkan).and_then(|window| {
                let device = VkDevice::new(&window, hidden).map_err(|error| error.to_string())?;
                Ok((window, device))
            });
            match opened {
                Ok((window, device)) => {
                    eprintln!("Infestation: renderer {}", device.description());
                    return Ok(Self::Hardware {
                        window,
                        device: Device::Vulkan(device),
                    });
                }
                Err(error) => {
                    eprintln!("Infestation: Vulkan unavailable ({error}); trying OpenGL");
                }
            }
        }
        if choice != RendererChoice::Software {
            let attributes = video.gl_attr();
            attributes.set_context_profile(GLProfile::Compatibility);
            attributes.set_context_version(2, 1);
            attributes.set_double_buffer(true);
            let opened = window(WindowKind::OpenGl).and_then(|window| {
                let device = GlDevice::new(&window).map_err(|error| error.to_string())?;
                Ok((window, device))
            });
            match opened {
                Ok((window, device)) => {
                    // A shown window presents on vertical blank, as retail's
                    // flip did; a hidden one never waits for a display.
                    let interval = if hidden {
                        SwapInterval::Immediate
                    } else {
                        SwapInterval::VSync
                    };
                    if let Err(error) = video.gl_set_swap_interval(interval) {
                        eprintln!("Infestation: OpenGL swap interval unavailable: {error}");
                    }
                    eprintln!("Infestation: renderer {}", device.description());
                    return Ok(Self::Hardware {
                        window,
                        device: Device::OpenGl(device),
                    });
                }
                Err(error) if choice != RendererChoice::OpenGl => {
                    eprintln!(
                        "Infestation: OpenGL unavailable ({error}); using the software renderer"
                    );
                }
                Err(error) => return Err(format!("OpenGL renderer unavailable: {error}")),
            }
        }
        window(WindowKind::Canvas)?
            .into_canvas()
            .software()
            .build()
            .map(Self::Canvas)
            .map_err(|error| error.to_string())
    }

    pub fn backend(&self) -> RendererBackend {
        match self {
            Self::Canvas(_) => RendererBackend::Software,
            Self::Hardware { device, .. } => device.backend(),
        }
    }

    /// Settings `+3C`'s Filtering for the device's own sampling (Vulkan's
    /// traced world); the other renderers read it from the scene's state.
    pub fn set_filtering(&self, level: u32) {
        if let Self::Hardware { device, .. } = self {
            device.set_filtering(level);
        }
    }

    /// The player's modern mode, for Vulkan's traced world.
    pub fn set_modern(&self, modern: crate::modern_settings::ModernSettings) {
        if let Self::Hardware { device, .. } = self {
            device.set_modern(modern);
        }
    }

    /// What the renderer runs of the modern mode, for its rows.
    pub fn modern_support(&self) -> crate::modern_settings::ModernSupport {
        match self {
            Self::Canvas(_) => crate::modern_settings::ModernSupport::default(),
            Self::Hardware { device, .. } => device.modern_support(),
        }
    }

    pub fn window(&self) -> &Window {
        match self {
            Self::Canvas(canvas) => canvas.window(),
            Self::Hardware { window, .. } => window,
        }
    }

    pub fn window_mut(&mut self) -> &mut Window {
        match self {
            Self::Canvas(canvas) => canvas.window_mut(),
            Self::Hardware { window, .. } => window,
        }
    }

    /// Physical output pixels.
    pub fn output_size(&self) -> Result<[u32; 2], String> {
        match self {
            Self::Canvas(canvas) => canvas.output_size().map(Into::into),
            Self::Hardware { window, device } => Ok(device.drawable_size(window)),
        }
    }

    pub fn texture_creator(&self) -> Option<TextureCreator<WindowContext>> {
        match self {
            Self::Canvas(canvas) => Some(canvas.texture_creator()),
            Self::Hardware { .. } => None,
        }
    }

    /// A native-size frame for this output's renderer, to be presented in
    /// `output` pixels with `scaling`. The CPU raster draws native pixels and
    /// presentation enlarges them; the OpenGL device draws the same frame at
    /// its presented size, which is the native size for authored output.
    pub fn frame(
        &self,
        dimensions: [u32; 2],
        output: [u32; 2],
        scaling: ClassicScaling,
        clear: [u8; 4],
    ) -> Result<SoftwareFrame, String> {
        match self {
            Self::Canvas(_) => Ok(SoftwareFrame::new(dimensions[0], dimensions[1], clear)),
            Self::Hardware { device, .. } => {
                let presented = scaling.destination(dimensions, output);
                SoftwareFrame::with_device(
                    device,
                    dimensions[0],
                    dimensions[1],
                    [presented.width, presented.height],
                    clear,
                )
                .map_err(|error| error.to_string())
            }
        }
    }

    /// Copy remembered top-down pixels into the selected output once, for
    /// a scaled snapshot, and read the whole output back. `framebuffer` is
    /// their size: native CPU pixels, or an OpenGL frame's presented size,
    /// which the copy then places one to one.
    pub fn export_scaled(
        &mut self,
        texture: Option<&mut Texture<'_>>,
        pixels: &[u8],
        framebuffer: [u32; 2],
        scaling: ClassicScaling,
    ) -> Result<([u32; 2], Vec<u8>), String> {
        match self {
            Self::Canvas(canvas) => {
                let texture = texture.ok_or("the SDL presentation texture is missing")?;
                let output = copy_pixels(canvas, texture, pixels, framebuffer, scaling)?;
                let pixels = canvas.read_pixels(None, PixelFormatEnum::RGBA32)?;
                Ok((output, pixels))
            }
            Self::Hardware { window, device } => {
                let output = device.drawable_size(window);
                Ok((output, scale_pixels(pixels, framebuffer, output, scaling)))
            }
        }
    }
}

/// Values stored by process/global model code, rather than the old renderer.
#[derive(Clone, Copy)]
pub(super) struct InheritedScene {
    pub environment: ModelEnvironment,
    pub primitive: PrimitiveState,
}

pub(super) struct SdlDisplayReset<'a, 't> {
    pub model_process: ModelProcessOwners,
    pub active: &'a ResidentRuntime,
    pub pending: &'a mut Option<PreparedDisplay<'t>>,
    pub output: &'a mut Output,
    pub creator: Option<&'t TextureCreator<WindowContext>>,
    pub tables: &'a PrimitiveTables,
    pub inherited: InheritedScene,
    pub successful_resets: &'a mut usize,
    pub scaling: ClassicScaling,
    /// A replay's recorded frame, kept while the display gives the frame
    /// the window opened with.
    pub frame_pin: Option<crate::presentation::FramePin>,
}

fn reset_error(error: impl std::fmt::Display) -> FrontendRuntimeError {
    FrontendRuntimeError::DisplayReset(error.to_string())
}

pub(super) fn display_devices(
    video: &sdl2::VideoSubsystem,
) -> Result<Vec<VideoDevice>, FrontendRuntimeError> {
    (0..video.num_video_displays().map_err(reset_error)?)
        .map(|id| {
            Ok(VideoDevice {
                id,
                // Authored fonts use byte characters. Ordinary Latin-1 names
                // retain their bytes; unavailable code points use a visible
                // replacement rather than passing UTF-8 continuation bytes.
                name: video
                    .display_name(id)
                    .map_err(reset_error)?
                    .chars()
                    .map(|character| u8::try_from(u32::from(character)).unwrap_or(b'?'))
                    .collect(),
            })
        })
        .collect()
}

/// What the monitor behind SDL output `display` offers. A monitor SDL
/// cannot describe offers no full-screen modes, and windows of any size.
pub(super) fn monitor_modes(video: &VideoSubsystem, display: i32) -> MonitorModes {
    let size = |mode: SdlMode| [mode.w as u32, mode.h as u32];
    let desktop = video.desktop_display_mode(display).ok().map(size);
    let count = video.num_display_modes(display).unwrap_or(0);
    MonitorModes::new(
        desktop,
        (0..count).filter_map(|index| video.display_mode(display, index).ok().map(size)),
    )
}

/// `00438AD0`'s startup search over `MonitorModes::startup_order`: the
/// first display the window takes is the startup display. The search stores
/// the chosen words without saving; the frontend factory's settings save
/// records them, as in retail. Retail's one-time notice (H0 44/45) answers
/// its DirectDraw error 0xA08, which SDL never raises, so the port's search
/// is silent apart from its log.
pub(super) fn start_display(
    window: &mut Window,
    display: i32,
    modes: &MonitorModes,
    saved: DisplaySelection,
) -> Result<(DisplaySelection, [u32; 2]), FrontendRuntimeError> {
    let mut failures = Vec::new();
    for candidate in modes.startup_order(saved) {
        match replace_window(window, display, candidate) {
            Ok(output) => return Ok((candidate, output)),
            Err(FrontendRuntimeError::DisplayReset(error)) => {
                eprintln!(
                    "Infestation: display {:?} {}x{} unavailable: {error}",
                    candidate.mode, candidate.size[0], candidate.size[1]
                );
                failures.push(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(FrontendRuntimeError::DisplayReset(format!(
        "no display started: {}",
        failures.join("; ")
    )))
}

/// The SDL streaming texture for a canvas; an OpenGL output needs none.
pub(super) fn texture<'t>(
    creator: Option<&'t TextureCreator<WindowContext>>,
    dimensions: [u32; 2],
) -> Result<Option<Texture<'t>>, FrontendRuntimeError> {
    let Some(creator) = creator else {
        return Ok(None);
    };
    let mut texture = creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, dimensions[0], dimensions[1])
        .map_err(reset_error)?;
    texture.set_blend_mode(sdl2::render::BlendMode::None);
    texture.set_scale_mode(sdl2::render::ScaleMode::Nearest);
    Ok(Some(texture))
}

/// Final completed-frame presentation, shared by all session screen classes.
/// Output bars are independent from the native H7 post-presentation clear.
pub(super) fn copy_framebuffer(
    output: &mut Output,
    texture: Option<&mut Texture<'_>>,
    frame: &mut SoftwareFrame,
    scaling: ClassicScaling,
) -> Result<[u32; 2], String> {
    let framebuffer = frame.buffer_size();
    match output {
        Output::Canvas(canvas) => {
            let texture = texture.ok_or("the SDL presentation texture is missing")?;
            copy_pixels(canvas, texture, frame.rgba(), framebuffer, scaling)
        }
        Output::Hardware { window, device } => {
            let size = device.drawable_size(window);
            if !frame.present_device(size, scaling.destination(framebuffer, size)) {
                return Err("a CPU frame cannot present through a hardware output".into());
            }
            Ok(size)
        }
    }
}

fn copy_pixels(
    canvas: &mut Canvas<Window>,
    texture: &mut Texture<'_>,
    pixels: &[u8],
    framebuffer: [u32; 2],
    scaling: ClassicScaling,
) -> Result<[u32; 2], String> {
    texture
        .update(None, pixels, framebuffer[0] as usize * 4)
        .map_err(|error| error.to_string())?;
    let output = canvas.output_size()?.into();
    let destination = scaling.destination(framebuffer, output);
    canvas.set_draw_color(sdl2::pixels::Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.copy(
        texture,
        None,
        sdl2::rect::Rect::new(
            destination.x,
            destination.y,
            destination.width,
            destination.height,
        ),
    )?;
    Ok(output)
}

/// Nearest-neighbour copy of top-down pixels into a black output, for an
/// OpenGL snapshot export. Each output pixel takes the source pixel under its
/// centre with the 16.16 steps of SDL's software scaler, so both renderers
/// export a frame the same way, ties included. (The GL window samples centres
/// too; ties there belong to the driver.)
fn scale_pixels(
    pixels: &[u8],
    framebuffer: [u32; 2],
    output: [u32; 2],
    scaling: ClassicScaling,
) -> Vec<u8> {
    let destination = scaling.destination(framebuffer, output);
    let centre = |index: u32, source: u32, target: u32| {
        let step = (u64::from(source) << 16) / u64::from(target);
        ((step / 2 + u64::from(index) * step) >> 16) as usize
    };
    let mut scaled = [0, 0, 0, 255].repeat(output[0] as usize * output[1] as usize);
    for y in 0..destination.height {
        let source_y = centre(y, framebuffer[1], destination.height);
        for x in 0..destination.width {
            let source_x = centre(x, framebuffer[0], destination.width);
            let source = (source_y * framebuffer[0] as usize + source_x) * 4;
            let target = ((destination.y as usize + y as usize) * output[0] as usize
                + destination.x as usize
                + x as usize)
                * 4;
            scaled[target..target + 4].copy_from_slice(&pixels[source..source + 4]);
        }
    }
    scaled
}

/// Read the actual full SDL target before Present invalidates its back buffer.
/// Timestamp publication immediately after Present; observation never crops.
pub(super) fn present_observed(
    output: &mut Output,
    capture: Option<&crate::session_capture::CaptureHandle>,
    phase: &'static str,
    world: Option<crate::session_capture::WorldPresentationState>,
) {
    let observed = capture.map(|capture| {
        let start = capture.elapsed_ns();
        let size = output.output_size()?;
        let pixels = match &*output {
            Output::Canvas(canvas) => canvas.read_pixels(None, PixelFormatEnum::RGBA32)?,
            Output::Hardware { device, .. } => device.read_output(size),
        };
        Ok::<_, String>((size, pixels, capture.elapsed_ns() - start))
    });
    match output {
        Output::Canvas(canvas) => canvas.present(),
        Output::Hardware { device, window } => device.swap(window),
    }
    if let (Some(capture), Some(observed)) = (capture, observed) {
        match observed {
            Ok((size, pixels, readback_ns)) => {
                capture.frame(size, pixels, phase, world, readback_ns)
            }
            Err(error) => {
                eprintln!("Infestation: capture readback failed: {error}");
                capture.readback_failed();
            }
        }
    }
}

impl FrontendDisplayHost for SdlDisplayReset<'_, '_> {
    fn residency(&self) -> &ResidentRuntime {
        self.pending
            .as_ref()
            .map_or(self.active, |pending| &pending.runtime)
    }

    fn monitor_modes(&self, device: i32) -> MonitorModes {
        monitor_modes(self.output.window().subsystem(), device)
    }

    fn set_modern(&mut self, modern: crate::modern_settings::ModernSettings) {
        self.output.set_modern(modern);
    }

    fn reset(
        &mut self,
        request: VideoResetRequest<'_>,
        display: DisplaySelection,
    ) -> Result<(), FrontendRuntimeError> {
        let tier_size = frontend_display_dimensions(request.variant).map_err(reset_error)?;
        let output = output_size(self.output.window().subsystem(), request.device, display)?;
        let mut native_frame = NativeFrame::for_output(tier_size, output, self.scaling);
        if let Some(pin) = self.frame_pin {
            native_frame = pin.apply(native_frame);
        }
        let dimensions = native_frame.size;
        // All allocations and fallible data conversion finish before touching
        // the physical window. Retained base banks keep their Arc identities.
        let mut runtime = self.residency().clone();
        let mut selection = runtime.summary().selection;
        selection.difficulty = request.variant as u8;
        runtime
            .apply(ResidencyRequest::Select { selection })
            .map_err(reset_error)?;
        let mut startup = frontend_startup(&runtime, native_frame).map_err(reset_error)?;
        apply_video_pipeline(request.words, &mut startup.scene).map_err(reset_error)?;
        self.output.set_filtering(
            request.words[crate::frontend_video::FILTERING_CELL]
                .min(crate::frontend_video::filtering_most(self.output.backend())),
        );
        let inherited = self.inherited;
        startup.scene.environment.view = inherited.environment.view;
        startup.scene.environment.coordinate_shift = inherited.environment.coordinate_shift;
        startup.scene.environment.fog_factor = inherited.environment.fog_factor;
        startup.scene.environment.lighting = inherited.environment.lighting;
        startup.scene.primitive_mode = inherited.primitive.mode;
        startup.scene.far_color = inherited.primitive.far_color;
        startup.scene.effect_color = inherited.primitive.effect_color;
        let mut frame = self
            .output
            .frame(dimensions, output, self.scaling, startup.clear_rgba)
            .map_err(reset_error)?;
        // Prove the exact fresh renderer configuration before acknowledging the
        // reset. Its resources borrow this candidate only within this scope.
        SoftwareScene::new(SoftwareSceneRequest {
            model_process: self.model_process.clone(),
            resources: RenderResources::new(&runtime),
            tables: self.tables,
            frame: &mut frame,
            config: startup.scene,
        })
        .map_err(reset_error)?;
        let texture = texture(self.creator, dimensions)?;
        replace_window(self.output.window_mut(), request.device, display)?;
        //A successfully admitted replacement constructor copies CURRENT
        //default slots once. Candidate validation and epoch resume never do.
        self.model_process
            .selected_renderer_constructed()
            .map_err(reset_error)?;
        *self.pending = Some(PreparedDisplay {
            runtime,
            frame,
            texture,
            startup,
            dimensions,
            output,
        });
        *self.successful_resets += 1;
        Ok(())
    }
}

/// The physical size `selection` gives the window: its resolution, or in
/// Borderless the desktop of SDL output `display`.
fn output_size(
    video: &VideoSubsystem,
    display: i32,
    selection: DisplaySelection,
) -> Result<[u32; 2], FrontendRuntimeError> {
    match selection.mode {
        DisplayMode::Window | DisplayMode::FullScreen => Ok(selection.size),
        DisplayMode::Borderless => {
            let desktop = video.desktop_display_mode(display).map_err(reset_error)?;
            Ok([desktop.w as u32, desktop.h as u32])
        }
    }
}

/// Give the window `selection` on SDL output `display` and return its
/// physical size: a centred window of the resolution, the monitor switched
/// to the resolution (only a reported mode, never SDL's closest), or a
/// borderless window over the desktop. A failed candidate restores the last
/// successful physical surface, including another candidate installed
/// earlier in the same input pass.
pub(super) fn replace_window(
    window: &mut Window,
    display: i32,
    selection: DisplaySelection,
) -> Result<[u32; 2], FrontendRuntimeError> {
    let old_size = window.size();
    let old_position = window.position();
    let old_fullscreen = window.fullscreen_state();
    let old_bordered = !is_borderless(window);
    let old_display = window.display_index().map_err(reset_error)?;
    let old_mode = window.display_mode().map_err(reset_error)?;
    let video = window.subsystem().clone();
    let dimensions = output_size(&video, display, selection)?;
    let result = (|| -> Result<(), String> {
        if selection.mode == DisplayMode::FullScreen
            && !monitor_modes(&video, display)
                .reported()
                .contains(&selection.size)
        {
            return Err(format!(
                "output display {display} reports no {}x{} mode",
                dimensions[0], dimensions[1]
            ));
        }
        window.set_fullscreen(FullscreenType::Off)?;
        // Leaving full screen restored the desktop mode, so the bounds are
        // the desktop's, not an exclusive mode's.
        let bounds = video.display_bounds(display)?;
        // Borderless drops the frame before sizing, so that even a hidden
        // window takes the whole desktop.
        window.set_bordered(selection.mode != DisplayMode::Borderless);
        window.set_position(
            WindowPos::Positioned(bounds.x() + (bounds.width() as i32 - dimensions[0] as i32) / 2),
            WindowPos::Positioned(bounds.y() + (bounds.height() as i32 - dimensions[1] as i32) / 2),
        );
        window
            .set_size(dimensions[0], dimensions[1])
            .map_err(|error| error.to_string())?;
        match selection.mode {
            DisplayMode::Window => {}
            DisplayMode::FullScreen => {
                set_window_display_mode(
                    window,
                    SdlMode {
                        format: PixelFormatEnum::Unknown,
                        w: dimensions[0] as i32,
                        h: dimensions[1] as i32,
                        refresh_rate: 0,
                    },
                )?;
                // SDL can select a closest available mode. The output policy
                // requires the requested physical pixels, so reject a different size.
                check_mode_dimensions(window.display_mode()?, dimensions)?;
                window.set_fullscreen(FullscreenType::True)?;
                // Hidden/minimized windows only store the fullscreen request;
                // SDL activates an exclusive monitor mode for a shown window.
                if exclusive_mode_is_active(window) {
                    check_mode_dimensions(video.current_display_mode(display)?, dimensions)?;
                }
            }
            // The desktop keeps its mode; the window covers its bounds.
            DisplayMode::Borderless => window.set_fullscreen(FullscreenType::Desktop)?,
        }
        let actual = window.size();
        if actual != (dimensions[0], dimensions[1]) {
            return Err(format!(
                "SDL selected surface {actual:?}, expected {dimensions:?}"
            ));
        }
        if window.display_index()? != display {
            return Err(format!("SDL did not select output display {display}"));
        }
        Ok(())
    })();
    if let Err(error) = result {
        let rollback = (|| -> Result<(), String> {
            window.set_fullscreen(FullscreenType::Off)?;
            window.set_bordered(old_bordered);
            set_window_display_mode(window, old_mode)?;
            window.set_position(old_position.0.into(), old_position.1.into());
            window
                .set_size(old_size.0, old_size.1)
                .map_err(|e| e.to_string())?;
            window.set_fullscreen(old_fullscreen)?;
            if exclusive_mode_is_active(window) {
                check_mode_dimensions(
                    window.subsystem().current_display_mode(old_display)?,
                    [old_mode.w as u32, old_mode.h as u32],
                )?;
            }
            if window.size() != old_size
                || window.fullscreen_state() != old_fullscreen
                || is_borderless(window) == old_bordered
                || window.display_index()? != old_display
            {
                return Err("SDL did not restore the prior surface".into());
            }
            Ok(())
        })();
        if let Err(rollback) = rollback {
            return Err(FrontendRuntimeError::DisplayRollback(format!(
                "{error}; rollback: {rollback}"
            )));
        }
        return Err(reset_error(error));
    }
    Ok(dimensions)
}

fn check_mode_dimensions(mode: SdlMode, expected: [u32; 2]) -> Result<(), String> {
    if [mode.w, mode.h] != expected.map(|value| value as i32) {
        return Err(format!(
            "SDL selected display mode {}x{}, expected {expected:?}",
            mode.w, mode.h
        ));
    }
    Ok(())
}

fn is_borderless(window: &Window) -> bool {
    window.window_flags() & sdl2::sys::SDL_WindowFlags::SDL_WINDOW_BORDERLESS as u32 != 0
}

fn exclusive_mode_is_active(window: &Window) -> bool {
    window.fullscreen_state() == FullscreenType::True
        && window.window_flags() & sdl2::sys::SDL_WindowFlags::SDL_WINDOW_SHOWN as u32 != 0
        && !window.is_minimized()
}

/// rust-sdl2 0.38's setter takes a pointer to `mode.to_ll()` inside a match
/// arm. That temporary expires before the FFI call. Keep the native descriptor
/// in this scope for both display replacement and rollback.
fn set_window_display_mode(window: &mut Window, mode: SdlMode) -> Result<(), String> {
    let native = mode.to_ll();
    // SAFETY: Window owns a live SDL window on its video thread. `native`
    // remains alive throughout this synchronous call, which copies the mode.
    let result = unsafe { sdl2::sys::SDL_SetWindowDisplayMode(window.raw(), &native) };
    if result < 0 {
        Err(sdl2::get_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "display_tests.rs"]
mod tests;

pub(super) fn add_scene_stats(total: &mut SoftwareSceneStats, next: SoftwareSceneStats) {
    // As with pixels, exhaustive fields keep every native consumer visible
    // when its renderer retires at residency/display changes or final report.
    let SoftwareSceneStats {
        retired_resource_batches,
        frame_begins,
        presentation_clears,
        begins,
        flushes,
        models,
        model_value_prefix_skips,
        object_group_prefix_closures,
        stars,
        footers,
        direct_sprites,
        direct_flat_quads,
        direct_shade_uv_quads,
        terrain_commands,
        water_commands,
        depth_resets,
        model_commands,
        rejected_primitives,
    } = next;
    total.retired_resource_batches += retired_resource_batches;
    total.frame_begins += frame_begins;
    total.presentation_clears += presentation_clears;
    total.begins += begins;
    total.flushes += flushes;
    total.models += models;
    total.model_value_prefix_skips += model_value_prefix_skips;
    total.object_group_prefix_closures += object_group_prefix_closures;
    total.stars += stars;
    total.footers += footers;
    total.direct_sprites += direct_sprites;
    total.direct_flat_quads += direct_flat_quads;
    total.direct_shade_uv_quads += direct_shade_uv_quads;
    total.terrain_commands += terrain_commands;
    total.water_commands += water_commands;
    total.depth_resets += depth_resets;
    total.model_commands += model_commands;
    total.rejected_primitives += rejected_primitives;
}

pub(super) fn add_pixel_stats(total: &mut SoftwarePixelStats, next: SoftwarePixelStats) {
    // A renderer retires at display replacement or final report publication.
    // Exhaustive fields keep new backend counters visible in both paths.
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
    } = next;
    total.triangles += triangles;
    total.solid_quads += solid_quads;
    total.solid_triangles += solid_triangles;
    total.textured_triangles += textured_triangles;
    total.shared_shade_far_triangles += shared_shade_far_triangles;
    total.textured_quads += textured_quads;
    total.ordinary_far_quads += ordinary_far_quads;
    total.textured_uv_triangles += textured_uv_triangles;
    total.textured_uv_quads += textured_uv_quads;
    total.shared_shade_uv_triangles += shared_shade_uv_triangles;
    total.shared_shade_uv_quads += shared_shade_uv_quads;
    total.effect_quads += effect_quads;
    total.d2_triangles += d2_triangles;
    total.d6_quads += d6_quads;
    total.textured_shade_uv_quads += textured_shade_uv_quads;
    total.colored_quads += colored_quads;
    total.da_triangles += da_triangles;
    total.flat_shade_quads += flat_shade_quads;
    total.skipped += skipped;
    total.fragments += fragments;
    total.unsupported += unsupported;
    total.traced += traced;
}
