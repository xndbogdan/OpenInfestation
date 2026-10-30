//! The player message's line and hint boxes, the `00413A70` controllers at
//! message +4 (type 1) and +8 (type 2). Each is a menu controller
//! (`00453380`) over one of two static four-row menus whose rows draw a
//! four-slot text ring (`00414BF0`), with a `0040D6A0` panel that frames and
//! zooms the box. `00413D70` adds a text and `00413F70` ages it.
//!
//! The two row arrays are static data natively, shared by every box of a
//! kind; the [`PlayerMessageMenus`] owners share their records the same way.

use thiserror::Error;

use crate::frontend_assets::{FrontendAssetError, FrontendAssets};
use crate::menu::{
    MenuActionRequest, MenuBasis, MenuCommand, MenuControllerSpec, MenuData, MenuError, MenuHost,
    MenuInput, MenuStack, MenuState, MenuVisibility,
};
use crate::menu_basis::MenuBasisMath;
use crate::panel_animation::PanelAnimation;
use crate::panel_frame::PanelFrame;

/// Rows `0052FB60` over layouts `0052FB20` (offline lines).
const LINE_ROWS: u32 = 0x0052_fb60;
const LINE_LAYOUTS: u32 = 0x0052_fb20;
/// Rows `0052FBE0` over layouts `0052FB00` (hints).
const HINT_ROWS: u32 = 0x0052_fbe0;
const HINT_LAYOUTS: u32 = 0x0052_fb00;

/// A row's draw argument 1 while it points at its box's text ring.
pub(crate) const RING_ATTACHED: u32 = 1;

/// The computer, "Shipboard CPU": definition `0052FD70`, seven rows at
/// `0052FC60`.
const COMPUTER_MENU: u32 = 0x0052_fd70;
/// Its row 6, "Cheats Menu", whose flags `00413410` rewrites.
const COMPUTER_CHEATS_ROW: usize = 6;

/// One entry of the computer's database (`00475320`): the eight-byte
/// records at `0053ABD8` below 0x95, then sixteen-byte ones at `0053B088`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DatabaseEntry {
    /// WORD +0: the H0 text, drawn from slot `text + 1`.
    pub text: u16,
    /// WORD +2: the H8 model, 0 for none.
    pub model: u16,
    /// BYTE +4: bit 2 leaves the entry out of the list (`004144E0`).
    pub flags: u8,
}

/// The database's entries `00414520` steps through: 1 to 0xAE.
pub const DATABASE_ENTRIES: u16 = 0xaf;
/// The hints `00414610` steps through: 0 to 0x48.
pub const HINT_COUNT: usize = 0x49;

/// The two static menus `004135D0` builds its boxes over, the computer
/// `00413410` shows, and the tables its database and hint list read.
#[derive(Clone, Debug)]
pub struct PlayerMessageMenus {
    pub lines: MenuData,
    pub hints: MenuData,
    /// Database entries 0 to 0xAE.
    pub database: Vec<DatabaseEntry>,
    /// `0053A27C`: each hint's H0 text (`0046D350`).
    pub hint_texts: Vec<u16>,
    /// The computer as `00413410` leaves its static rows: index 0 with the
    /// cheats row's flags 0 (settings `+84` has no bit outside `0xC0`),
    /// index 1 with them `| 1`.
    pub computer: [MenuData; 2],
}

impl PlayerMessageMenus {
    pub fn parse(assets: &FrontendAssets) -> Result<Self, FrontendAssetError> {
        let computer = |cheats: bool| -> Result<MenuData, FrontendAssetError> {
            let mut definition = assets.menu_definition(COMPUTER_MENU)?;
            let row = &mut definition.items[COMPUTER_CHEATS_ROW];
            row.flags = if cheats { row.flags | 1 } else { 0 };
            Ok(MenuData::new(definition))
        };
        let dword = |address: u32| {
            assets
                .data_dword(address)
                .ok_or(FrontendAssetError::DataAddress(address))
        };
        let database = (0..DATABASE_ENTRIES)
            .map(|index| {
                let address = if index < 0x95 {
                    0x0053_abd8 + 8 * u32::from(index)
                } else {
                    0x0053_b088 + 16 * u32::from(index - 0x95)
                };
                let [words, flags] = [dword(address)?, dword(address + 4)?];
                Ok(DatabaseEntry {
                    text: words as u16,
                    model: (words >> 16) as u16,
                    flags: flags as u8,
                })
            })
            .collect::<Result<_, FrontendAssetError>>()?;
        let hint_texts = (0..HINT_COUNT as u32)
            .map(|index| {
                let address = 0x0053_a27c + 2 * index;
                Ok((dword(address & !3)? >> (8 * (address & 3))) as u16)
            })
            .collect::<Result<_, FrontendAssetError>>()?;
        Ok(Self {
            lines: assets.menu_parts(LINE_ROWS, 4, LINE_LAYOUTS)?,
            hints: assets.menu_parts(HINT_ROWS, 4, HINT_LAYOUTS)?,
            database,
            hint_texts,
            computer: [computer(false)?, computer(true)?],
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MessageBoxKind {
    /// Type 1 at +4: flag-0 texts, top left.
    Lines,
    /// Type 2 at +8: flag-1 hints, centre.
    Hints,
}

#[derive(Debug, Error)]
pub enum MessageBoxError {
    #[error("message box menu: {0}")]
    Menu(String),
    #[error("message box has no current menu")]
    MissingMenu,
    #[error("online message boxes (0055F448 set) are not ported")]
    Network,
}

/// The services a box's menu reaches: navigation needs none, the tick
/// rotates the menu basis through `00491530`.
struct BoxMenuHost<'a> {
    math: Option<&'a MenuBasisMath>,
}

impl MenuHost for BoxMenuHost<'_> {
    type Error = String;

    fn action(&mut self, request: MenuActionRequest, _: &mut MenuStack) -> Result<(), String> {
        Err(format!(
            "message box rows have no action, but {:#010x} was reached",
            request.callback_address
        ))
    }

    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), String> {
        let math = self
            .math
            .ok_or("the message box tick needs the menu basis math")?;
        math.rotate(basis, angle);
        Ok(())
    }
}

fn menu_error<E: std::fmt::Debug>(error: MenuError<E>) -> MessageBoxError {
    MessageBoxError::Menu(format!("{error:?}"))
}

/// `max(3 s, 80 ms per byte)`, with the native unsigned product.
fn display_micros(text: &[u8]) -> i32 {
    let product = (text.len() as u32).wrapping_mul(80_000);
    if product < 3_000_000 {
        3_000_000
    } else {
        product as i32
    }
}

/// The C remainder of `selection + 3` by 4: the slot the last Next left.
fn previous_slot(selection: i32) -> usize {
    (selection.wrapping_add(3) % 4).rem_euclid(4) as usize
}

/// One `00413A70` box.
#[derive(Clone, Debug)]
pub struct MessageBox {
    pub kind: MessageBoxKind,
    pub stack: MenuStack,
    /// +30: the shown text's remaining display time.
    pub display_micros: i32,
    /// +34..+40: the text ring. The box's destructor (00413D00) frees it,
    /// then runs the base controller's 00453400.
    pub ring: [Option<Vec<u8>>; 4],
    /// +44: the next slot to fill.
    pub next: usize,
    /// +88/+8A: the frame's centre.
    pub center: [i16; 2],
    /// +8C..+9B: the frame panel.
    pub panel: PanelAnimation,
    /// +98: the panel's extent WORD.
    pub extent: u16,
    /// +9C: the frame flag (model 99 rather than 100).
    pub frame_flag: u8,
    /// +A0: the frame's rotation selector.
    pub rotation: u32,
}

impl MessageBox {
    /// `004135D0`'s new box: `00452310` over the static menu (resetting its
    /// row ages), the rows' ring arguments cleared, then `00413230`, which
    /// constructs the controller (`00413A70`) and pushes the menu.
    pub(crate) fn new(
        kind: MessageBoxKind,
        menus: &PlayerMessageMenus,
        network: u32,
    ) -> Result<Self, MessageBoxError> {
        if network != 0 {
            return Err(MessageBoxError::Network);
        }
        let (data, viewport) = match kind {
            MessageBoxKind::Lines => (menus.lines.clone(), [20, 0, 450, 500]),
            MessageBoxKind::Hints => (menus.hints.clone(), [300, 460, 810, 820]),
        };
        let menu = MenuState::new(data, MenuBasis::INITIAL);
        for item in menu.data().records_mut().items.iter_mut() {
            item.draw_arguments[1] = 0;
        }
        let hint = kind == MessageBoxKind::Hints;
        let [x0, y0, x1, y1] = viewport;
        let center = if hint {
            [x0, ((i32::from(y0) + i32::from(y1)) / 2) as i16]
        } else {
            [((i32::from(x0) + i32::from(x1)) / 2) as i16, y0]
        };
        let mut panel = PanelAnimation::with_duration(if hint { 0x4000 } else { 0x100 }, 150_000);
        // 00413A70's tail refills the exhausted timer and opens the box.
        panel.request(2);
        let mut stack = MenuStack::new(MenuControllerSpec {
            fade_rate: 0x10,
            viewport,
        });
        stack.push(menu);
        Ok(Self {
            kind,
            stack,
            display_micros: 0,
            ring: Default::default(),
            next: 0,
            center,
            panel,
            extent: if hint { 0xa000 } else { 0x5c00 },
            frame_flag: 1,
            rotation: if hint { 1 } else { 2 },
        })
    }

    /// The frame 00413A70 stores at +88..+A0: +9C selects slot 99 and the
    /// callback flag +9D is zero.
    pub fn panel_frame(&self) -> PanelFrame {
        PanelFrame {
            model_slot: PanelFrame::model_for(self.frame_flag),
            center_1024: self.center,
            maximum_extent_word: self.extent,
            flag: 0,
            orientation: self.rotation,
        }
    }

    fn selection(&self) -> Result<i32, MessageBoxError> {
        Ok(self
            .stack
            .current()
            .ok_or(MessageBoxError::MissingMenu)?
            .selection())
    }

    /// `004537E0(1, 0)`: Next on the current menu.
    fn next_row(&mut self, visibility: MenuVisibility) -> Result<(), MessageBoxError> {
        self.stack
            .command(
                MenuInput {
                    command: MenuCommand::Next,
                    visibility,
                    context: 0,
                },
                &mut BoxMenuHost { math: None },
            )
            .map(|_| ())
            .map_err(menu_error)
    }

    /// `00413D70`.
    pub(crate) fn add_text(
        &mut self,
        text: &[u8],
        visibility: MenuVisibility,
    ) -> Result<(), MessageBoxError> {
        // Virtual +08, 004151D0, activates the controller.
        self.stack.set_active(true);
        self.panel.request(2);
        if self.display_micros == 0 {
            self.display_micros = display_micros(text);
        }
        if self.next == previous_slot(self.selection()?) {
            self.next_row(visibility)?;
            self.display_micros = display_micros(text);
        } else if self.kind == MessageBoxKind::Lines {
            self.panel.requested_scale_word = self.panel.requested_scale_word.wrapping_add(0x1600);
        }
        let slot = self.next;
        self.ring[slot] = Some(text.to_vec());
        {
            let menu = self.stack.current().ok_or(MessageBoxError::MissingMenu)?;
            let mut records = menu.data().records_mut();
            let item = records
                .items
                .get_mut(slot)
                .ok_or(MessageBoxError::MissingMenu)?;
            item.draw_arguments[1] = RING_ATTACHED;
            item.draw_arguments[0] = slot as u32;
            item.age = 0;
        }
        self.next = (slot + 1) & 3;
        Ok(())
    }

    /// Virtual +04, `004151C0`: the controller fades out.
    pub(crate) fn deactivate(&mut self) {
        self.stack.set_active(false);
    }

    /// `00413F70`: the shown text's time runs out into the next row, which
    /// starts its own time or, when empty, closes the box. Then the panel
    /// (`004554E0`) and the controller (`00453560`) tick; a fully faded
    /// controller returns false.
    pub(crate) fn update(
        &mut self,
        delta_micros: i32,
        visibility: MenuVisibility,
        math: &MenuBasisMath,
    ) -> Result<bool, MessageBoxError> {
        if self.display_micros != 0 {
            if delta_micros < self.display_micros {
                self.display_micros = self.display_micros.wrapping_sub(delta_micros);
            } else {
                let left = previous_slot(self.selection()?);
                self.display_micros = 0;
                if self.kind == MessageBoxKind::Lines {
                    self.panel.requested_scale_word =
                        self.panel.requested_scale_word.wrapping_sub(0x1600);
                }
                self.next_row(visibility)?;
                let shown = usize::try_from(self.selection()?)
                    .ok()
                    .and_then(|slot| self.ring.get(slot))
                    .ok_or(MessageBoxError::MissingMenu)?;
                match shown {
                    None => {
                        self.deactivate();
                        self.panel.request(0);
                    }
                    Some(text) => self.display_micros = display_micros(text),
                }
                self.ring[left] = None;
            }
        }
        self.panel.tick(delta_micros);
        self.stack
            .tick(delta_micros, &mut BoxMenuHost { math: Some(math) })
            .map_err(menu_error)
    }
}

/// The boxes a player's message holds natively at +4 and +8.
#[derive(Clone, Debug, Default)]
pub struct PlayerMessageBoxes {
    pub lines: Option<MessageBox>,
    pub hints: Option<MessageBox>,
}

/// Process values the message reaches when it delivers or updates.
#[derive(Clone, Debug)]
pub struct PlayerMessageEnvironment {
    pub menus: PlayerMessageMenus,
    pub math: MenuBasisMath,
    /// 0055F448.
    pub network: u32,
    /// 00453360 over G+5C2.
    pub visibility: MenuVisibility,
    /// Settings DWORD +78 is 1: a hint pauses the game.
    pub pause_on_hints: bool,
    /// 0055F14C.
    pub attract: bool,
    /// 004391B0()+64C, the UI state.
    pub ui_state: u32,
    /// The Cheats Menu's process bytes, which the session owns.
    pub cheats: ComputerCheats,
}

/// The process bytes the Cheats Menu's rows write and the computer reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComputerCheats {
    /// `00546A04`, Show Full Database's byte: the database lists every entry
    /// and Select grants it.
    pub full_database: bool,
    /// `0052F880`, the world Skip to goes to, 2 to 24; the executable's
    /// data starts it at 2.
    pub skip_world: u32,
}

/// UI requests the message makes through `004391B0`/`00438380`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerMessageEffect {
    /// A hint asks for UI state 6, the paused game, or an expired hint under
    /// state 6 asks for state 4 back.
    RequestUiState(u32),
}

#[cfg(test)]
#[path = "message_widgets_tests.rs"]
mod tests;
