//! Outer frontend presentation at 00484940, including 00439270 and its narrow
//! native 0x301 catch. Required effects execute on explicitly selected renderer
//! identities; this owner does not implement or claim a pixel backend.

use thiserror::Error;

use crate::frontend_control::{FrontendControl, FrontendControlError};
use crate::frontend_scene::{
    draw_footer, FrontendScene, FrontendSceneHost, SceneCommand, SceneDrawOutcome, SceneFrame,
    SceneProjection, SceneView,
};
use crate::menu::{MenuDrawHost, MenuDrawInput, MenuDrawOutcome, MenuVisibility};
use crate::screen_renderer_preparation::{prepare_screen_renderer, ScreenRendererPreparationHost};

#[derive(Clone, Copy, Debug)]
pub struct FrontendFrameInput<'a> {
    /// Actual base screen +C0, owned by ScreenInput; milliseconds.
    pub age_millis: i32,
    pub visibility: MenuVisibility,
    pub sine_quarter: &'a [u16; 4096],
}

#[derive(Clone, Copy, Debug)]
pub struct FrontendSceneEnvironment {
    pub projection: SceneProjection,
    /// The renderer's current view, before this frame publishes a camera.
    pub inherited_view: SceneView,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameResourceState {
    OutOfRange,
    Vacant,
    Resident,
}

/// A frame host retains the captured caller-context identity, using its live
/// fields in scene setup and MenuDrawHost operations. It must preserve unrelated
/// context fields. Renderer identities are stable owned handles, never native
/// pointer values. No operation has a successful default implementation.
pub trait FrontendFrameHost:
    MenuDrawHost + ScreenRendererPreparationHost<<Self as MenuDrawHost>::Error>
{
    /// Capture session+648 AFTER 00439270; retain the context identity for this
    /// call, not an invented replacement context or a snapshot of its fields.
    fn capture_caller_context(&mut self) -> Result<(), Self::Error>;
    /// Renderer virtual +14(0), on the outer frame's captured renderer.
    fn reset_renderer(&mut self, renderer: Self::Renderer) -> Result<(), Self::Error>;
    /// 00495770 checks the actual surface/window state. A needed rebuild raises
    /// native 0x711; it is not a successful validation or a 0x301 recovery.
    fn validate_surface(&mut self) -> Result<(), Self::Error>;
    fn scene_environment(&mut self) -> Result<FrontendSceneEnvironment, Self::Error>;
    /// Actual current surface width/height at 00495900/00495910.
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error>;
    /// Resolve resources and execute the command on this selected renderer.
    fn scene_submit(
        &mut self,
        renderer: Self::Renderer,
        command: SceneCommand,
    ) -> Result<(), Self::Error>;
    fn footer_style_word(&mut self) -> Result<u32, Self::Error>;
    fn sprite_dimensions(&mut self, slot: u16) -> Result<[u16; 2], Self::Error>;
    /// The main menu's scene is about to draw: the modern mode may trace it.
    fn scene_starting(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    /// The scene's models are all submitted, before the footer and the
    /// flush: a device that traces rays draws them now (the modern mode).
    fn scene_submitted(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    /// Read the live resource registry. OutOfRange means the slot exceeds the
    /// native inclusive bound; a vacant in-range slot needs its lazy loader.
    fn resource_state(&mut self, handler: u8, slot: u16)
        -> Result<FrameResourceState, Self::Error>;
    fn load_resource(&mut self, handler: u8, slot: u16) -> Result<(), Self::Error>;
    /// Only errors carrying an actual native code may return Some. Unsupported
    /// Rust adapters must not be relabelled 0x301 to manufacture frame success.
    fn native_error_code(error: &Self::Error) -> Option<u32>;
    /// 004923C0 destroys the caught error through its own virtual destructor.
    fn dispose_caught_error(&mut self, error: Self::Error) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontendFrameOutcome {
    Drawn {
        scene: SceneDrawOutcome,
        menu: Option<MenuDrawOutcome>,
    },
    /// Native 0x301 skipped the rest of the guarded body; tail checks ran.
    RecoveredNative301,
}

#[derive(Debug, Error)]
pub enum FrontendFrameError<E> {
    #[error("required frontend frame operation failed")]
    Host(E),
    #[error("frontend menu draw failed: {0}")]
    Control(FrontendControlError<E>),
    #[error("required frontend tail resource H{handler}/{slot} is out of range")]
    RequiredResource { handler: u8, slot: u16 },
}

/// Execute the full outer frame. Errors preserve completed publications and
/// submissions. The owning screen stops on uncaught failure; only native 0x301
/// raised inside the guarded body permits another frame. Resource-tail failures
/// and all 00439270 failures are outside that catch, even when their code is301.
pub fn draw_frontend_frame<H: FrontendFrameHost>(
    control: &mut FrontendControl,
    scene: &FrontendScene,
    input: FrontendFrameInput<'_>,
    host: &mut H,
) -> Result<FrontendFrameOutcome, FrontendFrameError<H::Error>> {
    if control.stopped() {
        return Err(FrontendFrameError::Control(FrontendControlError::Stopped));
    }
    // 00439270 precedes the native error scope. Do not coalesce its lookups.
    prepare_screen_renderer(host).map_err(FrontendFrameError::Host)?;
    host.capture_caller_context()
        .map_err(FrontendFrameError::Host)?;

    // Rust lexical ownership replaces the native setjmp frame; no fake
    // renderer begin/end operation is substituted for that bookkeeping.
    let outcome = match draw_guarded(control, scene, input, host) {
        Ok(outcome) => outcome,
        Err(FrontendFrameError::Host(error))
        | Err(FrontendFrameError::Control(FrontendControlError::Host(error)))
            if H::native_error_code(&error) == Some(0x301) && !control.stopped() =>
        {
            host.dispose_caught_error(error)
                .map_err(FrontendFrameError::Host)?;
            FrontendFrameOutcome::RecoveredNative301
        }
        Err(error) => return Err(error),
    };

    // These are lazy registration checks, not palette/font-state restoration.
    // H4/0 is unconditional; H6/3 is tested only if its inclusive bound allows3.
    for (handler, slot) in [(4, 0), (6, 3)] {
        match host
            .resource_state(handler, slot)
            .map_err(FrontendFrameError::Host)?
        {
            FrameResourceState::Resident => {}
            FrameResourceState::Vacant => host
                .load_resource(handler, slot)
                .map_err(FrontendFrameError::Host)?,
            FrameResourceState::OutOfRange if handler == 6 => {}
            FrameResourceState::OutOfRange => {
                return Err(FrontendFrameError::RequiredResource { handler, slot });
            }
        }
    }
    Ok(outcome)
}

fn draw_guarded<H: FrontendFrameHost>(
    control: &mut FrontendControl,
    scene: &FrontendScene,
    input: FrontendFrameInput<'_>,
    host: &mut H,
) -> Result<FrontendFrameOutcome, FrontendFrameError<H::Error>> {
    let footer_renderer = host.renderer().map_err(FrontendFrameError::Host)?;
    host.scene_submit(footer_renderer, SceneCommand::BeginBatch)
        .map_err(FrontendFrameError::Host)?;
    host.reset_renderer(footer_renderer)
        .map_err(FrontendFrameError::Host)?;
    host.validate_surface().map_err(FrontendFrameError::Host)?;

    let scene_renderer = host.renderer().map_err(FrontendFrameError::Host)?;
    let environment = host.scene_environment().map_err(FrontendFrameError::Host)?;
    host.scene_starting().map_err(FrontendFrameError::Host)?;
    let scene = scene
        .draw_scene(
            SceneFrame {
                projection: environment.projection,
                inherited_view: environment.inherited_view,
                camera_eye: control.camera().eye().position,
                camera_look_at: control.camera().look_at().position,
                animation: control.animation(),
                age_millis: input.age_millis,
                exit_millis: control.timer_0120_millis(),
            },
            &mut SceneAdapter {
                host,
                renderer: scene_renderer,
            },
        )
        .map_err(FrontendFrameError::Host)?;
    host.scene_submitted().map_err(FrontendFrameError::Host)?;
    draw_footer(&mut SceneAdapter {
        host,
        renderer: footer_renderer,
    })
    .map_err(FrontendFrameError::Host)?;
    let renderer = host.renderer().map_err(FrontendFrameError::Host)?;
    host.scene_submit(renderer, SceneCommand::FlushBatch)
        .map_err(FrontendFrameError::Host)?;

    let menu = if control.camera().ready() {
        let surface_pixels = host.surface_pixels().map_err(FrontendFrameError::Host)?;
        control
            .draw_menu_catching(
                MenuDrawInput {
                    surface_pixels,
                    visibility: input.visibility,
                    sine_quarter: input.sine_quarter,
                },
                host,
                H::native_error_code,
            )
            .map_err(FrontendFrameError::Control)?
    } else {
        None
    };
    Ok(FrontendFrameOutcome::Drawn { scene, menu })
}

struct SceneAdapter<'a, H: FrontendFrameHost> {
    host: &'a mut H,
    renderer: H::Renderer,
}

impl<H: FrontendFrameHost> FrontendSceneHost for SceneAdapter<'_, H> {
    type Error = H::Error;
    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        self.host.scene_submit(self.renderer, command)
    }
    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        self.host.footer_style_word()
    }
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        self.host.surface_pixels()
    }
    fn sprite_dimensions(&mut self, slot: u16) -> Result<[u16; 2], Self::Error> {
        self.host.sprite_dimensions(slot)
    }
}

#[cfg(test)]
#[path = "frontend_frame_tests.rs"]
mod tests;
