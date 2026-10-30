//! The message's shown child +0C: the type-3 controller `004140A0` that
//! `004137D0` creates or reactivates through `00413230(3, flag, menu)` and
//! pushes menus onto (`00453440`). Gameplay shows the factory shop here.
//!
//! `00414140`'s flag picks the viewport: 1 the full 1024 units (the
//! briefing's own controller), 0 the shop's (470, 0)..(980, 700), which the
//! controller's draw `004141B0` frames with a panel. The controller fades at
//! 0x20; its tick `00414180` adds the delta's milliseconds to `+30` before
//! the base tick `00453560`. Virtual +4 (`004151C0`) fades it out and +8
//! (`004151D0`) back in. When the fade reaches zero, `00413870` destroys it
//! (`00414110`, the base controller's `00453400`) with its menus and clears
//! `+0C` and the shown byte; a shop menu's
//! destructor `004315E0` then undocks its factory, unless a rebuild replaced
//! that menu (`+60`).
//!
//! The phase-5 in-world menu (`004433C0`) and the computer (`00413410`)
//! show here too; their rows' actions reach the session, which holds the
//! menu definitions, the settings and the phase requests.

use crate::menu::{
    MenuActionRequest, MenuBasis, MenuControllerSpec, MenuError, MenuHost, MenuStack, MenuState,
};
use crate::menu_basis::MenuBasisMath;
use crate::object_runtime::ObjectId;

/// `004140A0`'s fade rate.
pub const SHOWN_MENU_FADE_RATE: i32 = 0x20;

/// `00414140`'s viewports.
pub fn shown_menu_viewport(flag: u8) -> [i16; 4] {
    if flag != 0 {
        [0, 0, 0x400, 0x400]
    } else {
        [0x1d6, 0, 0x3d4, 700]
    }
}

/// Who built a shown menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShownMenuOwner {
    /// A factory shop menu (`00431000`) and its `+5C` factory.
    Shop { factory: ObjectId },
    /// The phase-5 in-world menu (`004433C0`, vtable `00523440`): its
    /// destructor `00443400` requests `return_phase`, 6 as 4.
    InWorld { return_phase: u32 },
    /// A plain `00452310` menu, whose destructor `00452420` only unlinks it
    /// (`+30/+34`): the computer and the children `00453BE0` pushes.
    Plain,
    /// The computer's database (`00414420`, vtable `005207F4`) at entry
    /// `+5C`; its destructor `00414480` requests phase 4.
    Database { entry: u16 },
    /// The computer's hint list (vtable `00520808`) at hint `+5C`; its
    /// destructor `004147D0` requests phase 4.
    HintList { hint: i32 },
}

/// `00414520`, the database's command: its Select under the full database
/// cheat (`00546A04`) grants the shown entry (`004750C0`, then
/// `0046B2B0(entry, 8)`). The arena hands it to the session as an action
/// whose argument 0 is the entry; the database's rows have no action.
pub const DATABASE_SELECT: u32 = 0x0041_4520;

/// A row action of a menu the arena does not own, with the menu's owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShownMenuAction {
    pub player: crate::object_runtime::PlayerId,
    pub owner: ShownMenuOwner,
    pub request: MenuActionRequest,
}

#[derive(Clone, Debug)]
pub struct ShownMenu {
    pub stack: MenuStack,
    /// `+30`: the milliseconds the controller's tick accumulates.
    pub clock_30: i32,
    /// The stack's menus' owners, oldest first.
    owners: Vec<ShownMenuOwner>,
}

impl ShownMenu {
    /// `004140A0(flag)`.
    pub(super) fn new(flag: u8) -> Self {
        Self {
            stack: MenuStack::new(MenuControllerSpec {
                fade_rate: SHOWN_MENU_FADE_RATE,
                viewport: shown_menu_viewport(flag),
            }),
            clock_30: 0,
            owners: Vec::new(),
        }
    }

    /// The current menu's owner.
    pub fn current_owner(&self) -> Option<ShownMenuOwner> {
        self.owners.last().copied()
    }

    /// The stack's menus' owners, oldest first.
    pub fn owners(&self) -> &[ShownMenuOwner] {
        &self.owners
    }

    /// The current menu's owner, to update its state.
    pub(crate) fn current_owner_mut(&mut self) -> Option<&mut ShownMenuOwner> {
        self.owners.last_mut()
    }

    /// `00453440`.
    pub(super) fn push(&mut self, menu: MenuState, owner: ShownMenuOwner) {
        self.stack.push(menu);
        self.owners.push(owner);
    }

    /// `00453480`: the current menu is destroyed (as replaced, so its
    /// destructor does nothing) and the new one linked.
    pub(super) fn replace(&mut self, menu: MenuState, owner: ShownMenuOwner) {
        self.stack.replace(menu);
        self.owners.pop();
        self.owners.push(owner);
    }

    /// `004534D0(count)`: the newest menus go; their owners wait for their
    /// destructors. Returns them and whether a menu remains (`00453540`).
    pub(super) fn pop(&mut self, count: i32) -> (Vec<ShownMenuOwner>, bool) {
        let mut popped = Vec::new();
        for _ in 0..count.max(0) {
            match self.owners.pop() {
                Some(owner) => popped.push(owner),
                None => break,
            }
        }
        self.stack.pop(count);
        (popped, !self.stack.is_empty())
    }

    /// The owners of the menus a destroyed controller takes with it.
    pub(super) fn into_owners(self) -> Vec<ShownMenuOwner> {
        self.owners
    }

    /// `00414180`: `+30` takes the delta's milliseconds, then `00453560`.
    /// Returns false when the fade reached zero.
    pub(super) fn tick(
        &mut self,
        delta_micros: i32,
        math: &MenuBasisMath,
    ) -> Result<bool, MenuError<String>> {
        self.clock_30 = self.clock_30.wrapping_add(delta_micros / 1000);
        self.stack
            .tick(delta_micros, &mut ShownMenuTickHost { math })
    }
}

/// The tick's services: the base tick rotates each menu's basis through
/// `00491530`; no row action is reachable from it.
struct ShownMenuTickHost<'a> {
    math: &'a MenuBasisMath,
}

impl MenuHost for ShownMenuTickHost<'_> {
    type Error = String;

    fn action(&mut self, request: MenuActionRequest, _: &mut MenuStack) -> Result<(), String> {
        Err(format!(
            "the shown menu's tick reached row action {:#010x}",
            request.callback_address
        ))
    }

    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), String> {
        self.math.rotate(basis, angle);
        Ok(())
    }
}

#[cfg(test)]
#[path = "shown_menu_tests.rs"]
mod tests;
