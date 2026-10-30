//! Concrete camera wrapper + authored row execution at 004849DC..00484A05.

use super::{
    flatten_menu_error, select_camera, FrontendCamera, FrontendControl, FrontendControlConfig,
    FrontendControlError, FrontendState,
};
use crate::menu::{
    MenuBasis, MenuControllerDrawSetup, MenuDrawColors, MenuDrawHost, MenuDrawInput,
    MenuDrawOutcome, MenuError, MenuRowDrawRequest, MenuState,
};
use crate::profile_menu::{ProfileMenuState, ProfileTimer};
use crate::profile_store::ProfileStore;

impl FrontendControl {
    /// Draw the current authored menu when the camera gate opens. The host
    /// performs context setup and real row rendering; row-height feedback,
    /// clipping, palette, timing, and frontend camera wrappers execute here.
    /// Scene/background/footer rendering precedes this boundary in retail and
    /// remains the enclosing screen's responsibility.
    pub fn draw_menu<H: MenuDrawHost>(
        &mut self,
        input: MenuDrawInput<'_>,
        host: &mut H,
    ) -> Result<Option<MenuDrawOutcome>, FrontendControlError<H::Error>> {
        self.draw_menu_catching(input, host, |_| None)
    }

    /// Only the full frame owns the native 0x301 catch. Standalone menu draws
    /// retain their fail-stop policy; this entry preserves partial mutations.
    pub(crate) fn draw_menu_catching<H: MenuDrawHost>(
        &mut self,
        input: MenuDrawInput<'_>,
        host: &mut H,
        native_error_code: fn(&H::Error) -> Option<u32>,
    ) -> Result<Option<MenuDrawOutcome>, FrontendControlError<H::Error>> {
        self.draw_menu_if_ready(native_error_code, |control| {
            let mut adapter = DrawAdapter {
                config: &control.config,
                camera: &mut control.camera,
                state: &mut control.state,
                host,
            };
            control
                .menus
                .draw_catching(input, &mut adapter, |error| match error {
                    FrontendControlError::Host(error) => native_error_code(error),
                    _ => None,
                })
                .map_err(flatten_menu_error)
        })
    }
}

pub(super) fn prepare_row<E>(
    config: &FrontendControlConfig,
    camera: &mut FrontendCamera,
    state: &mut FrontendState,
    callback: u32,
    arguments: [u32; 2],
) -> Result<u32, FrontendControlError<E>> {
    match callback {
        0x0048_35e0 => {
            select_camera(config, camera, state, 0)?;
            Ok(0x0048_34f0)
        }
        // 0045F520 -> 00485320. The owning screen is a phase-2 frontend.
        0x0045_f520 => {
            select_camera(config, camera, state, arguments[1])?;
            Ok(0x0045_3a80)
        }
        other => Ok(other),
    }
}

struct DrawAdapter<'a, H> {
    config: &'a FrontendControlConfig,
    camera: &'a mut FrontendCamera,
    state: &'a mut FrontendState,
    host: &'a mut H,
}

impl<H: MenuDrawHost> MenuDrawHost for DrawAdapter<'_, H> {
    type Error = FrontendControlError<H::Error>;

    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        self.host
            .begin_controller(setup)
            .map_err(FrontendControlError::Host)
    }
    fn publish_menu_basis(&mut self, basis: MenuBasis) -> Result<(), Self::Error> {
        self.host
            .publish_menu_basis(basis)
            .map_err(FrontendControlError::Host)
    }
    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        self.host.clip_rect().map_err(FrontendControlError::Host)
    }
    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error> {
        self.host
            .set_clip_rect(clip)
            .map_err(FrontendControlError::Host)
    }
    fn colors(&self) -> MenuDrawColors {
        self.host.colors()
    }

    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        menu: &mut MenuState,
    ) -> Result<(), Self::Error> {
        // Keep the caller's provenance intact while dispatching the wrapper's
        // actual renderer callback. Every renderer-mutated descriptor field is
        // returned, including the signed Y used for shared row-height writes.
        let mut delegated = *request;
        delegated.callback_address = prepare_row(
            self.config,
            self.camera,
            self.state,
            request.callback_address,
            request.arguments,
        )?;
        self.host
            .draw_row(&mut delegated, menu)
            .map_err(FrontendControlError::Host)?;
        request.descriptor = delegated.descriptor;
        Ok(())
    }

    fn begin_profile_draw(&mut self) -> Result<(), MenuError<Self::Error>> {
        self.host.begin_profile_draw().map_err(lift_menu_error)
    }

    fn profile_store(&mut self) -> Result<&mut ProfileStore, MenuError<Self::Error>> {
        self.host.profile_store().map_err(lift_menu_error)
    }

    fn draw_profile_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        base: &mut MenuState,
        state: &mut ProfileMenuState,
    ) -> Result<(), MenuError<Self::Error>> {
        if request.callback_address == 0x0044_8e60 {
            // 00485320(1) runs after the header's font0/style3 resolution.
            // begin_profile_draw has already resolved these same resources;
            // their residency is immutable for this complete draw call.
            select_camera(self.config, self.camera, self.state, 1).map_err(MenuError::Host)?;
        }
        self.host
            .draw_profile_row(request, base, state)
            .map_err(lift_menu_error)
    }

    fn draw_profile_status(
        &mut self,
        timer: ProfileTimer,
        surface: [i32; 2],
    ) -> Result<(), MenuError<Self::Error>> {
        self.host
            .draw_profile_status(timer, surface)
            .map_err(lift_menu_error)
    }

    fn draw_text_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        text: &[u8],
    ) -> Result<(), MenuError<Self::Error>> {
        self.host
            .draw_text_row(request, text)
            .map_err(lift_menu_error)
    }
}

fn lift_menu_error<E>(error: MenuError<E>) -> MenuError<FrontendControlError<E>> {
    match error {
        MenuError::State(error) => MenuError::State(error),
        MenuError::ProfileStore(error) => MenuError::ProfileStore(error),
        MenuError::Host(error) => MenuError::Host(FrontendControlError::Host(error)),
    }
}

#[cfg(test)]
#[path = "frontend_draw_tests.rs"]
mod tests;
