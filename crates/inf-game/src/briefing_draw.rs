//! Briefing's custom row `0040A350` on the native font renderer.
//!
//! The authored menus own position, extent, clipping, reveal age, selection
//! color and row-height feedback through the shared menu walker. This adapter
//! changes only the custom callback's signed H0 gate and bit-20 clear.

use crate::{
    menu::{
        MenuBasis, MenuControllerDrawSetup, MenuDrawColors, MenuDrawHost, MenuDrawInput,
        MenuDrawOutcome, MenuError, MenuRowDrawRequest, MenuStack, MenuState,
    },
    menu_text::{MenuRowTextRequest, MenuTextPlacement, MenuTextResources},
    render_resources::{RenderResourceError, RenderResources},
    software_menu::{SoftwareMenu, SoftwareMenuError},
};

pub const BRIEFING_TEXT_CALLBACK: u32 = 0x0040_a350;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BriefingMenuDrawOutcome {
    pub menu: MenuDrawOutcome,
    /// Native missing H0 skips the row. The platform caller can log these
    /// resource omissions once without refusing the entire briefing phase.
    pub omitted_string_slots: Vec<i16>,
}

pub fn draw_briefing_menu<'m, 'f, 'r>(
    menus: &mut MenuStack,
    input: MenuDrawInput<'_>,
    menu: &mut SoftwareMenu<'m, 'f, 'r>,
    resources: RenderResources<'r>,
) -> Result<BriefingMenuDrawOutcome, MenuError<SoftwareMenuError>> {
    let mut host = BriefingMenuDrawHost {
        menu,
        resources,
        omitted_string_slots: Vec::new(),
    };
    let outcome = menus.draw(input, &mut host)?;
    Ok(BriefingMenuDrawOutcome {
        menu: outcome,
        omitted_string_slots: host.omitted_string_slots,
    })
}

struct BriefingMenuDrawHost<'a, 'm, 'f, 'r> {
    menu: &'a mut SoftwareMenu<'m, 'f, 'r>,
    resources: RenderResources<'r>,
    omitted_string_slots: Vec<i16>,
}

impl MenuDrawHost for BriefingMenuDrawHost<'_, '_, '_, '_> {
    type Error = SoftwareMenuError;

    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        self.menu.begin_controller(setup)
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
        if request.callback_address != BRIEFING_TEXT_CALLBACK {
            return self.menu.draw_row(request, state);
        }
        // 00452C3C publishes the row projection before entering its callback,
        // even when 0040A350 subsequently skips an invalid/missing H0 slot.
        self.menu
            .publish_row_origin(request.context_origin_pixels)?;
        let Some(argument) = custom_text_argument(request.arguments[0]) else {
            return Ok(());
        };
        let text = match self.resources.string(argument) {
            Ok(text) => text,
            Err(RenderResourceError::NotResident { .. }) => {
                self.omitted_string_slots.push(argument as i16);
                return Ok(());
            }
            Err(error) => {
                return Err(SoftwareMenuError::Text(
                    crate::menu_text::MenuTextError::Resource(error),
                ));
            }
        };
        // 0040A396..0040A39C clears the native row descriptor, so this
        // mutation remains visible to the caller's row-height feedback.
        request.descriptor.flags &= !0x20;
        self.menu.draw_row_text(MenuRowTextRequest {
            descriptor: &mut request.descriptor,
            text,
            placement: MenuTextPlacement::Left,
            context_origin_pixels: request.context_origin_pixels,
        })?;
        Ok(())
    }
}

fn custom_text_argument(argument: u32) -> Option<i32> {
    (argument != 0 && argument as i16 >= 0).then_some(argument as i32)
}

#[cfg(test)]
#[path = "briefing_draw_tests.rs"]
mod tests;
