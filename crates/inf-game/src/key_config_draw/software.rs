//! The key configuration screen on the CPU scene.

use thiserror::Error;

use super::*;
use crate::{
    frontend_renderer::{FrontendRenderContext, FrontendRendererError, FrontendRendererState},
    frontend_screen::FrontendProcessState,
    input::InputContextId,
    key_config_screen::KeyConfigScreen,
    menu::{
        MenuBasis, MenuControllerDrawSetup, MenuDrawColors, MenuDrawHost, MenuDrawInput,
        MenuDrawOutcome, MenuError, MenuState, MenuVisibility,
    },
    menu_text::MenuRowTextRequest,
    render_resources::{RenderResourceError, RenderResources},
    scene_resources::SceneResourceBackend,
    screen_registry::{RegistryError, ScreenAttachment, ScreenRegistry},
    screen_renderer_preparation::{prepare_screen_renderer, ScenePreparation},
    software_menu::{SoftwareMenu, SoftwareMenuError, SoftwareMenuRequest},
    software_scene::{SoftwareScene, SoftwareSceneError},
};

#[derive(Debug, Error)]
pub enum KeyConfigDrawError {
    #[error("key configuration preamble: {0}")]
    Preamble(#[from] FrontendRendererError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error(transparent)]
    Menu(#[from] SoftwareMenuError),
    #[error("key configuration binding row: {0}")]
    Row(#[from] BindingRowError<RenderResourceError>),
    #[error("key configuration menu: {0}")]
    MenuState(String),
    #[error("key configuration screen is not ready to draw")]
    NotReady,
}

/// `00453610` rows over the shared software menu, with the binding row.
struct KeyConfigMenuDraw<'m, 'a, 'f, 'r> {
    menu: &'m mut SoftwareMenu<'a, 'f, 'r>,
    resources: RenderResources<'r>,
    tables: &'m KeyBindingTables,
    bindings: &'m KeyBindings,
    capture_rows: [bool; KEY_BINDING_ROWS],
    clock: u32,
}

impl MenuDrawHost for KeyConfigMenuDraw<'_, '_, '_, '_> {
    type Error = KeyConfigDrawError;

    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        Ok(self.menu.begin_controller(setup)?)
    }

    fn publish_menu_basis(&mut self, basis: MenuBasis) -> Result<(), Self::Error> {
        Ok(self.menu.publish_menu_basis(basis)?)
    }

    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        Ok(self.menu.clip_rect()?)
    }

    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error> {
        Ok(self.menu.set_clip_rect(clip)?)
    }

    fn colors(&self) -> MenuDrawColors {
        self.menu.colors()
    }

    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        menu: &mut MenuState,
    ) -> Result<(), Self::Error> {
        if request.callback_address != BINDING_ROW_CALLBACK {
            return Ok(self.menu.draw_row(request, menu)?);
        }
        let capturing = usize::try_from(request.arguments[0])
            .ok()
            .and_then(|row| self.capture_rows.get(row).copied())
            .unwrap_or(false);
        let plan = binding_row_plan(
            request,
            BindingRowState {
                tables: self.tables,
                bindings: self.bindings,
                capturing,
                clock: self.clock,
            },
            &self.resources,
        )?;
        // 004452BB works on its own copy; the caller's Y is never updated.
        let mut local = plan.local;
        for (placement, text) in plan.first_sides() {
            self.menu.draw_row_text(MenuRowTextRequest {
                descriptor: &mut local,
                text,
                placement,
                context_origin_pixels: request.context_origin_pixels,
            })?;
        }
        if let Some(text) = plan.joystick.as_deref() {
            local.color = request.descriptor.color;
            local.extent_pixels[0] = request.descriptor.extent_pixels[0];
            self.menu.draw_row_text(MenuRowTextRequest {
                descriptor: &mut local,
                text,
                placement: MenuTextPlacement::Right,
                context_origin_pixels: request.context_origin_pixels,
            })?;
        }
        Ok(())
    }
}

/// Everything `004450B0` reads besides the screen.
pub(crate) struct KeyConfigDrawRequest<'a, 'f, 'r> {
    pub scene: &'a mut SoftwareScene<'f, 'r>,
    pub resources: RenderResources<'r>,
    pub registry: &'a ScreenRegistry<FrontendRenderContext, InputContextId>,
    pub attachment: ScreenAttachment,
    pub process: &'a FrontendProcessState,
    pub published: &'a mut FrontendRendererState,
    pub tables: &'a KeyBindingTables,
    pub bindings: &'a KeyBindings,
    pub visibility: MenuVisibility,
    pub sine_quarter: &'a [u16; 4096],
}

/// Linked +08 `004450B0` over the CPU scene.
pub(crate) fn draw(
    request: KeyConfigDrawRequest<'_, '_, '_>,
    screen: &mut KeyConfigScreen,
) -> Result<MenuDrawOutcome, KeyConfigDrawError> {
    let KeyConfigDrawRequest {
        scene,
        resources,
        registry,
        attachment,
        process,
        published,
        tables,
        bindings,
        visibility,
        sine_quarter,
    } = request;
    prepare_screen_renderer(&mut ScenePreparation {
        scene: &mut *scene,
        state: &mut *published,
    })?;
    // 004957F0 unlocks a locked surface; the CPU frame has no lock.
    let focal = registry.backend(attachment.context_owner)?.projection.focal;
    let surface_pixels = scene.surface_pixels()?;
    let capture_rows = std::array::from_fn(|row| screen.row_capturing(row));
    let mut menu = SoftwareMenu::new(SoftwareMenuRequest {
        scene,
        resources,
        focal,
        process,
        state: &mut published.menu,
    })?;
    let mut host = KeyConfigMenuDraw {
        menu: &mut menu,
        resources,
        tables,
        bindings,
        capture_rows,
        clock: process.animation().value(),
    };
    let menus = screen.menus_mut().ok_or(KeyConfigDrawError::NotReady)?;
    menus
        .draw(
            MenuDrawInput {
                surface_pixels,
                visibility,
                sine_quarter,
            },
            &mut host,
        )
        .map_err(|error| match error {
            MenuError::Host(error) => error,
            other => KeyConfigDrawError::MenuState(other.to_string()),
        })
}
