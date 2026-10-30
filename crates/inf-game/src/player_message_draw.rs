//! A player message box's draw on the native font renderer. `00413820`
//! draws each live box with the controller draw `00453610`, whose virtual
//! +0C (`00413F30`) frames the box with its panel (`00455080`) in a batch
//! of its own. The rows' callback `00414BF0` draws the box's ring text in
//! the row's slot with `00453810`, left-aligned.
//!
//! `00413F30` skips the frame online (`0055F448`); online boxes are never
//! built, so every drawn box is framed.

use crate::{
    frontend_camera::AnimationClock,
    frontend_scene::SceneProjection,
    menu::{
        MenuBasis, MenuControllerDrawSetup, MenuDrawColors, MenuDrawHost, MenuDrawInput,
        MenuDrawOutcome, MenuError, MenuRowDrawRequest, MenuState,
    },
    menu_basis::MenuBasisMath,
    menu_text::{MenuRowTextRequest, MenuTextPlacement},
    object_runtime::MessageBox,
    panel_animation::PanelAnimation,
    panel_frame::{draw_panel_frame, PanelFrame},
    scene_resources::{
        ResidentSceneHost, ResolvedSceneCommand, SceneResourceBackend, SceneResourceError,
    },
    software_menu::{SoftwareMenu, SoftwareMenuError},
    software_scene::SoftwareSceneError,
};

/// Row callback `00414BF0`.
pub const MESSAGE_ROW_CALLBACK: u32 = 0x0041_4bf0;

/// What the frame reads besides the box's panel.
#[derive(Clone, Copy)]
pub struct MessageFrameInputs<'a> {
    /// The context template `0043B4A0` copies, at the live surface size.
    pub projection: SceneProjection,
    pub animation: AnimationClock,
    pub math: &'a MenuBasisMath,
}

/// `00453610` over one box.
pub fn draw_message_box(
    message_box: &mut MessageBox,
    input: MenuDrawInput<'_>,
    menu: &mut SoftwareMenu<'_, '_, '_>,
    frame: MessageFrameInputs<'_>,
) -> Result<MenuDrawOutcome, MenuError<SoftwareMenuError>> {
    let shape = message_box.panel_frame();
    let MessageBox {
        stack, ring, panel, ..
    } = message_box;
    stack.draw(
        input,
        &mut MessageBoxDrawHost {
            menu,
            ring,
            shape,
            panel: *panel,
            frame,
        },
    )
}

struct MessageBoxDrawHost<'a, 'm, 'f, 'r> {
    menu: &'a mut SoftwareMenu<'m, 'f, 'r>,
    ring: &'a [Option<Vec<u8>>; 4],
    shape: PanelFrame,
    panel: PanelAnimation,
    frame: MessageFrameInputs<'a>,
}

fn scene_error(error: SceneResourceError<SoftwareSceneError>) -> SoftwareMenuError {
    match error {
        SceneResourceError::Resource(error) => SoftwareSceneError::Resource(error).into(),
        SceneResourceError::Backend(error) => error.into(),
    }
}

impl MenuDrawHost for MessageBoxDrawHost<'_, '_, '_, '_> {
    type Error = SoftwareMenuError;

    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        self.menu.begin_controller(setup)
    }

    /// `00413F30`: `004A4400`, the frame, then `004A4460`.
    fn draw_controller_frame(&mut self) -> Result<(), Self::Error> {
        self.menu
            .scene_mut()
            .submit(ResolvedSceneCommand::BeginBatch)?;
        let resources = self.menu.resources();
        draw_panel_frame(
            self.shape,
            self.panel,
            self.frame.projection,
            self.frame.animation,
            self.frame.math,
            &mut ResidentSceneHost {
                resources,
                backend: self.menu.scene_mut(),
            },
        )
        .map_err(scene_error)?;
        self.menu
            .scene_mut()
            .submit(ResolvedSceneCommand::FlushBatch)?;
        Ok(())
    }

    fn publish_menu_basis(&mut self, basis: MenuBasis) -> Result<(), Self::Error> {
        self.menu.publish_menu_basis(basis)
    }

    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        self.menu.clip_rect()
    }

    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error> {
        self.menu.set_clip_rect(clip)
    }

    fn colors(&self) -> MenuDrawColors {
        self.menu.colors()
    }

    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        state: &mut MenuState,
    ) -> Result<(), Self::Error> {
        if request.callback_address != MESSAGE_ROW_CALLBACK {
            return self.menu.draw_row(request, state);
        }
        // 00452C3C publishes the row projection before the callback, even
        // when 00414BF0 then draws nothing.
        self.menu
            .publish_row_origin(request.context_origin_pixels)?;
        // Argument 1 points at the ring natively; argument 0 is the slot.
        if request.arguments[1] == 0 {
            return Ok(());
        }
        let slot = request.arguments[0] as usize;
        let text = self.ring.get(slot).ok_or(SoftwareMenuError::RingSlot {
            callback: MESSAGE_ROW_CALLBACK,
            slot,
        })?;
        let Some(text) = text else {
            return Ok(());
        };
        self.menu.draw_row_text(MenuRowTextRequest {
            descriptor: &mut request.descriptor,
            text,
            placement: MenuTextPlacement::Left,
            context_origin_pixels: request.context_origin_pixels,
        })?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "player_message_draw_tests.rs"]
mod tests;
