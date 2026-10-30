//! The two distinct briefing menu controllers: player type 3 (`004140A0`)
//! and the screen's text controller (`0040A420`, screen +2FC).
//!
//! Detail and list definitions alias the first scratch row at 0052F010.
//! Only one text menu is live. Copying that row before replacement preserves
//! its mutable height/age/arguments across the two authored array views.

use crate::{
    briefing_runtime::{BriefingRuntime, BriefingTickEffect},
    frontend_assets::{FrontendAssetError, FrontendAssets},
    menu::{
        MenuActionRequest, MenuBasis, MenuCommand, MenuControllerSpec, MenuData, MenuError,
        MenuHost, MenuInput, MenuStack, MenuState, MenuTickPolicy, MenuVisibility,
    },
    menu_basis::MenuBasisMath,
};
use inf_formats::{
    BRIEFING_DETAIL_MENU_ADDRESS, BRIEFING_LIST_MENU_ADDRESS, BRIEFING_MENU_ADDRESS,
};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct BriefingMenuTemplates {
    root: MenuData,
    detail: MenuData,
    list: MenuData,
}

impl BriefingMenuTemplates {
    pub fn parse(assets: &FrontendAssets) -> Result<Self, FrontendAssetError> {
        Ok(Self {
            root: MenuData::new(assets.menu_definition(BRIEFING_MENU_ADDRESS)?),
            detail: MenuData::new(assets.menu_definition(BRIEFING_DETAIL_MENU_ADDRESS)?),
            list: MenuData::new(assets.menu_definition(BRIEFING_LIST_MENU_ADDRESS)?),
        })
    }
}

#[derive(Debug)]
pub struct BriefingControls {
    pub root: MenuStack,
    pub text: MenuStack,
    templates: BriefingMenuTemplates,
    list_active: bool,
}

#[derive(Debug, Error)]
pub enum BriefingControlError {
    #[error("briefing menu callback {0:#010x} has no native owner")]
    Action(u32),
}

impl BriefingControls {
    pub fn new(templates: BriefingMenuTemplates, mission_selection: u32) -> Self {
        // 0040A420 patches only the root header's string argument.
        templates.root.records_mut().items[0].draw_arguments[0] =
            mission_selection.wrapping_add(0xe0);
        // 004137D0 supplies flag 1 to 00413230(type 3), whose constructor
        // 004140A0/00414140 therefore chooses the full 1024-unit viewport.
        let mut root = MenuStack::new(MenuControllerSpec {
            fade_rate: 32,
            viewport: [0, 0, 1024, 1024],
        });
        root.push(MenuState::new(templates.root.clone(), MenuBasis::INITIAL));
        let mut text = MenuStack::new(MenuControllerSpec {
            fade_rate: 65535,
            viewport: [100, 640, 740, 1100],
        });
        templates.detail.records_mut().items[0].draw_arguments[0] = 0;
        text.push(
            MenuState::new(templates.detail.clone(), MenuBasis::INITIAL)
                .with_tick_policy(MenuTickPolicy::BriefingDetail),
        );
        Self {
            root,
            text,
            templates,
            list_active: false,
        }
    }

    /// Native +2FC drawing ticks its controller separately from the player
    /// controller. Native DWORD 0xFFFF is positive 65535, so a normal
    /// scheduler tick reaches full fade through the ordinary menu owner.
    pub fn tick_text(
        &mut self,
        delta: i32,
        basis: &MenuBasisMath,
    ) -> Result<(), MenuError<BriefingControlError>> {
        let mut host = ControlHost {
            runtime: None,
            templates: &self.templates,
            text: None,
            list_active: &mut self.list_active,
            basis,
        };
        self.text.tick(delta, &mut host)?;
        Ok(())
    }

    pub fn command(
        &mut self,
        command: MenuCommand,
        runtime: &mut BriefingRuntime,
        basis: &MenuBasisMath,
    ) -> Result<(), MenuError<BriefingControlError>> {
        let mut host = ControlHost {
            runtime: Some(runtime),
            templates: &self.templates,
            text: Some(&mut self.text),
            list_active: &mut self.list_active,
            basis,
        };
        self.root.command(
            MenuInput {
                command,
                visibility: MenuVisibility::OwnerFlagSet,
                context: crate::briefing_screen::BRIEFING_CALLBACK_CONTEXT,
            },
            &mut host,
        )?;
        Ok(())
    }

    pub fn apply_effect(
        &mut self,
        effect: &BriefingTickEffect,
        runtime: &mut BriefingRuntime,
        basis: &MenuBasisMath,
    ) -> Result<(), MenuError<BriefingControlError>> {
        match *effect {
            BriefingTickEffect::ShowList => self.replace_text(true),
            BriefingTickEffect::TickTextController { delta_micros } => {
                self.tick_text(delta_micros, basis)?
            }
            BriefingTickEffect::RevealListRows { rows } => self.visible_list_rows(rows),
            BriefingTickEffect::ControllerCommand { command: 1, .. } => {
                self.command(MenuCommand::Next, runtime, basis)?
            }
            BriefingTickEffect::TickController { delta_micros } => {
                let mut host = ControlHost {
                    runtime: Some(runtime),
                    templates: &self.templates,
                    text: Some(&mut self.text),
                    list_active: &mut self.list_active,
                    basis,
                };
                self.root.tick(delta_micros, &mut host)?;
            }
            BriefingTickEffect::RecordActivated { .. } => {
                if runtime
                    .current_record()
                    .is_some_and(|record| record.raw_kind == 0)
                {
                    // 0040CE40 publishes screen +2A0 only for kind 0, as
                    // command 1 to the text menu (004537E0): the detail
                    // menu's 0040D5B0 stores it in its row's +8 and clears
                    // +C (authored 0), then runs the base 00452E20.
                    // 0040D1E0 clears the display scratch 0052F018 without
                    // clearing that predecessor: camera/visibility records
                    // must not republish the preceding detail's text.
                    if let Some(id) = runtime.menu_text_id() {
                        self.templates.detail.records_mut().items[0].draw_arguments[0] =
                            u32::from(id);
                        self.templates.list.records_mut().items[0].draw_arguments[0] =
                            u32::from(id);
                    }
                    if let Some(menu) = self.text.current_mut() {
                        menu.reset_ages();
                    }
                }
            }
            BriefingTickEffect::Complete(_) => self.root.pop(1),
            _ => {}
        }
        Ok(())
    }

    pub fn visible_list_rows(&mut self, rows: u8) {
        if self.list_active {
            let mut records = self.templates.list.records_mut();
            records.layouts[0].visible_rows = rows;
            // 0040CD49 clears the next row's age, retaining every authored
            // row flag. The fourth reveal's native write lies beyond the
            // four declared rows and has no in-bounds row consumer here.
            if let Some(row) = records.items.get_mut(usize::from(rows)) {
                row.age = 0;
            }
        }
    }

    fn replace_text(&mut self, list: bool) {
        replace_text(&mut self.text, &self.templates, &mut self.list_active, list);
    }
}

fn replace_text(
    text: &mut MenuStack,
    templates: &BriefingMenuTemplates,
    list_active: &mut bool,
    list: bool,
) {
    // Preserve the aliased first row before the native replacement destroys
    // the old view; MenuState::new then resets ages just as 00452310 does.
    if *list_active {
        let first = templates.list.records().items[0].clone();
        templates.detail.records_mut().items[0] = first;
    } else {
        let first = templates.detail.records().items[0].clone();
        templates.list.records_mut().items[0] = first;
    }
    let data = if list {
        let mut records = templates.list.records_mut();
        for (index, row) in records.items.iter_mut().enumerate() {
            row.draw_arguments = [0x380 + index as u32, 0];
        }
        drop(records);
        templates.list.clone()
    } else {
        templates.detail.records_mut().items[0].draw_arguments[0] = 0;
        templates.detail.clone()
    };
    let policy = if list {
        MenuTickPolicy::Base
    } else {
        MenuTickPolicy::BriefingDetail
    };
    text.replace(MenuState::new(data, MenuBasis::INITIAL).with_tick_policy(policy));
    *list_active = list;
}

#[cfg(test)]
#[path = "briefing_controls_tests.rs"]
mod tests;

struct ControlHost<'a> {
    runtime: Option<&'a mut BriefingRuntime>,
    templates: &'a BriefingMenuTemplates,
    text: Option<&'a mut MenuStack>,
    list_active: &'a mut bool,
    basis: &'a MenuBasisMath,
}

impl MenuHost for ControlHost<'_> {
    type Error = BriefingControlError;
    fn action(
        &mut self,
        request: MenuActionRequest,
        _stack: &mut MenuStack,
    ) -> Result<(), Self::Error> {
        let Some(runtime) = self.runtime.as_deref_mut() else {
            return Err(BriefingControlError::Action(request.callback_address));
        };
        match request.callback_address {
            0x0040_a3f0 => {
                runtime.show_detail();
                replace_text(
                    self.text
                        .as_deref_mut()
                        .expect("root action text controller"),
                    self.templates,
                    self.list_active,
                    false,
                );
            }
            0x0040_a400 => {
                runtime.show_list();
                replace_text(
                    self.text
                        .as_deref_mut()
                        .expect("root action text controller"),
                    self.templates,
                    self.list_active,
                    true,
                );
            }
            0x0040_a410 => {
                runtime.arm_start();
            }
            address => return Err(BriefingControlError::Action(address)),
        }
        Ok(())
    }
    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error> {
        self.basis.rotate(basis, angle);
        Ok(())
    }
}
