//! The network menus ([NETWORK_RUNTIME.md](../../../docs/re/NETWORK_RUNTIME.md),
//! "Network menus"). The root's Network row (`0045FD40`) opens `0045F9C0`'s
//! list of connections (vtable `0052621C`). Choosing one asks for the host's
//! address, as DirectPlay's TCP/IP dialog did, then opens the list of
//! sessions (`0045F9C0(1)`, vtable `00526208`): "Host session on this
//! machine", then the sessions a search found, rebuilt each second
//! (`0045FDE0`). Both lists draw through their `+0C`, `0045FDC0`, which
//! hands its four arguments to the shared row walk `00452570`, as every
//! menu here draws. Hosting opens the authored settings menu `00538650`, whose
//! Start runs `0045F320`; joining opens the wait menu `005386A0` (vtable
//! `005261F4`, tick `0045FF80`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use inf_formats::{
    parse_frontend_menu_items, FrontendError, FrontendExecutable, FrontendExecutableError,
    FrontendMenuItem,
};
use thiserror::Error;

use crate::menu::{
    MenuActionRequest, MenuBasis, MenuCommand, MenuData, MenuDerivedData, MenuLayouts, MenuOwner,
    MenuStack, MenuState, MenuStateError,
};
use crate::network::udp::{SearchTarget, SessionInfo};

pub const LIST_CONSTRUCTOR: u32 = 0x0045_f9c0;
const CONNECTIONS_VTABLE: u32 = 0x0052_621c;
const SESSIONS_VTABLE: u32 = 0x0052_6208;
/// The address prompt has no retail object: it stands in for DirectPlay's
/// TCP/IP dialog.
const ADDRESS_VTABLE: u32 = 0;
/// `0045F700`: a list row, its entry's name, else "No more".
pub const NAME_ROW: u32 = 0x0045_f700;
/// `0045F740`: a list row's command.
pub const LIST_ACTION: u32 = 0x0045_f740;
/// The authored host settings (`00538650`), the same rows laid out again
/// after an arena change (`00538668`), and the wait (`005386A0`).
pub const HOST_MENU: u32 = 0x0053_8650;
pub const HOST_MENU_RELAID: u32 = 0x0053_8668;
pub const WAIT_MENU: u32 = 0x0053_86a0;
/// The host menu's callbacks: the arena row's command (`0045F560`), a value
/// row's (`0045F440`), the winning score's (`0045F480`), and Start Session
/// (`0045F320`).
pub const ARENA_ACTION: u32 = 0x0045_f560;
pub const VALUE_ACTION: u32 = 0x0045_f440;
pub const LIMIT_ACTION: u32 = 0x0045_f480;
pub const START_ACTION: u32 = 0x0045_f320;
/// `00453C50`, the host menu's Back.
pub const BACK_ACTION: u32 = 0x0045_3c50;
/// `0045FB50`: the list holds eight sessions.
pub const SESSIONS: usize = 8;
/// `0045FA9B`: at most four of the list's rows show at once.
const VISIBLE_ROWS: u8 = 4;
/// `0045FDE0`: the sessions are searched again each second.
const REFRESH_MICROS: i32 = 1_000_000;
/// `0045F740`'s host row (`005384C1`/`C2`): the host menu's nine rows under
/// ten visible.
const HOST_ROWS: (u8, u8) = (9, 10);
/// `0045FB50`'s entries hold a name of `0x40` bytes.
const NAME_BYTES: usize = 0x3f;
/// `005387AC`.
const NO_MORE: &str = "No more";
/// The address prompt's line.
const ADDRESS_LABEL: &str = "Host address (blank: local network): ";

/// The host menu's draws: the arena row (`0045EE20`) and the Winning
/// Score's (`0045F280`).
pub const ARENA_ROW: u32 = 0x0045_ee20;
pub const SCORE_ROW_DRAW: u32 = 0x0045_f280;
/// `00463AB0`'s settings object.
pub const SETTINGS_ADDRESS: u32 = 0x0054_e8a8;
/// Settings offsets the host menu edits (`00463AB0`'s object at `0054E8A8`).
pub const SETTING_MODE: u32 = 0x10;
pub const SETTING_ARENA: u32 = 0x1c;
pub const SETTING_WINNING_SCORE: u32 = 0x30;
pub const SETTING_CHEATS: u32 = 0x84;
/// `0045F670`'s rows: Recharging grenades, Catchup Scale and Winning Score.
const GRENADES_ROW: usize = 5;
const CATCHUP_ROW: usize = 6;
const SCORE_ROW: usize = 7;
const ROW_SHOWN: u32 = 4;
/// `0045F670` sets settings +28 (Recharging grenades) in mode 3.
const SETTING_GRENADES: u32 = 0x28;

/// `00488A40`..`00488A80`'s table (`0053C570`, eleven arenas of `0x18`
/// bytes): a name (or the world's, H0 `0xE0` + world), the world, two words,
/// the mode, and the preview sprite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arena {
    pub name: Option<&'static str>,
    pub world: u32,
    pub word_08: u32,
    pub word_0c: u32,
    pub mode: u32,
    pub sprite: u16,
}

pub const ARENAS: [Arena; 11] = [
    arena(None, 25, 3, 2, 4, 0x723),
    arena(Some("Buggyball"), 25, 1, 2, 4, 0x71a),
    arena(None, 33, 3, 2, 0, 0x71e),
    arena(Some("Aerial Banner"), 33, 5, 2, 0, 0x719),
    arena(None, 32, 5, 2, 0, 0x720),
    arena(None, 27, 1, 8, 2, 0x71b),
    arena(None, 31, 1, 8, 2, 0x71c),
    arena(None, 26, 5, 4, 0, 0x71f),
    arena(None, 28, 5, 8, 3, 0x721),
    arena(None, 29, 5, 8, 3, 0x722),
    arena(Some("Buggy Race"), 28, 1, 8, 3, 0x71d),
];

const fn arena(
    name: Option<&'static str>,
    world: u32,
    word_08: u32,
    word_0c: u32,
    mode: u32,
    sprite: u16,
) -> Arena {
    Arena {
        name,
        world,
        word_08,
        word_0c,
        mode,
        sprite,
    }
}

/// `0045EE20`'s previews, in draw order: each arena's sprite (the table's
/// `+14`, as the draw reads it at `005383BC`) as a direct textured quad
/// (`+9C`, packet word 4) at depth 0x40, centred on an arc about (W/2,
/// 55% of H). Arena `i` lies at angle `(0x50000 + 0x10000 i) / 40`, 45%
/// of the surface out for even `i` and 35% for odd, and spans W/20 each
/// way; the selected one spans W/16, pulsing by W/80 with the animation
/// clock (`005471C0 << 11`), and is drawn twice. The eleventh, Buggy Race,
/// needs the cheat word's bit 7. Every product and quotient is the
/// native signed 32-bit one, each offset its low WORD.
pub fn arena_previews(
    surface: [i32; 2],
    selected: u32,
    clock: u32,
    eleven: bool,
    sine_quarter: &[u16; 4096],
) -> Vec<crate::scene_sprite::DirectSpriteQuad> {
    use crate::startup_draw::table_sine;
    let [width, height] = surface;
    let (cx, cy) = (width / 2, height.wrapping_mul(55) / 100);
    let mut quads = Vec::new();
    for (index, arena) in ARENAS.iter().enumerate() {
        if index == 10 && !eleven {
            break;
        }
        let half = if index as u32 == selected {
            let pulse = table_sine(sine_quarter, clock.wrapping_shl(11) as i32);
            width.wrapping_mul(5) / 80 + i32::from(((width / 80).wrapping_mul(pulse) >> 15) as i16)
        } else {
            width / 20
        };
        let reach = if index % 2 == 0 { -45 } else { -35 };
        let angle = ((0x50000 + 0x10000 * index as u32) / 40) as i32;
        let offset = |size: i32, phase: i32| {
            i32::from(
                ((size.wrapping_mul(reach) / 100).wrapping_mul(table_sine(sine_quarter, phase))
                    >> 15) as i16,
            )
        };
        let x = cx + offset(width, angle.wrapping_add(0x4000));
        let y = cy + offset(height, angle);
        let quad = crate::scene_sprite::DirectSpriteQuad {
            sprite_slot: arena.sprite,
            corners: [
                [x - half, y - half],
                [x + half, y - half],
                [x + half, y + half],
                [x - half, y + half],
            ]
            .map(|corner| corner.map(|value| value as i16)),
            vertex_depths: [0x40; 4],
            descriptor_word: 4,
            mode: crate::scene_sprite::DirectSpriteMode::Textured,
        };
        quads.push(quad);
        if index as u32 == selected {
            quads.push(quad);
        }
    }
    quads
}

/// `00488A60`..: the arena `index % 11`.
pub fn arena_at(index: u32) -> Arena {
    ARENAS[(index % 11) as usize]
}

/// `004889C0`: the arena's own name, else the H0 text of its world's.
pub fn arena_name(index: u32) -> Result<&'static str, i32> {
    let arena = arena_at(index);
    arena.name.ok_or(0xe0 + arena.world as i32)
}

#[derive(Debug, Error)]
pub enum NetworkMenuError<E> {
    #[error(transparent)]
    State(#[from] MenuStateError),
    #[error("network menu service failed")]
    Host(E),
    #[error("network menu row {0} has no entry")]
    Row(u32),
}

/// The four template rows `0045F9C0` copies: the header (`005386F8`), the
/// host row (`005386B8`), a session row (`005386D8`) and a connection row
/// (`00538718`).
#[derive(Clone, Debug)]
pub struct NetworkMenuTemplates {
    header: FrontendMenuItem,
    host: FrontendMenuItem,
    session: FrontendMenuItem,
    connection: FrontendMenuItem,
}

#[derive(Debug, Error)]
pub enum NetworkTemplateError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error(transparent)]
    Frontend(#[from] FrontendError),
}

impl NetworkMenuTemplates {
    pub fn parse(executable: &[u8]) -> Result<Self, NetworkTemplateError> {
        let data = FrontendExecutable::parse(executable)?.data();
        let row = |address| -> Result<FrontendMenuItem, FrontendError> {
            Ok(parse_frontend_menu_items(data, address, 1)?.remove(0))
        };
        Ok(Self {
            header: row(0x0053_86f8)?,
            host: row(0x0053_86b8)?,
            session: row(0x0053_86d8)?,
            connection: row(0x0053_8718)?,
        })
    }
}

/// A request to close the session, which a network menu dropped without
/// moving forward makes (retail's destructors `0045FBE0`, `0045FC50`,
/// `0045FCB0` and `0045FF20` call `00493650`; the scalar deleting forms
/// `0045FBC0`, `0045FD20` and `0045F9A0` run three of them). The link takes
/// it on its next frame.
pub type CloseToken = Arc<AtomicBool>;

/// What the network services answer the menus.
pub trait NetworkMenuServices {
    type Error;
    /// The link's close request, for the menus it opens.
    fn close_token(&mut self) -> Result<CloseToken, Self::Error>;
    /// Whether the network game after phase 9 is ported. Until it is, Start
    /// Session closes the session again and stays in the menu.
    fn network_game_ready(&mut self) -> bool;
    fn rotate_menu_basis(&mut self, basis: &mut MenuBasis, angle: i32) -> Result<(), Self::Error>;
    /// The shared authored menu at `address`.
    fn menu_data(&mut self, address: u32) -> Result<MenuData, Self::Error>;
    /// `00493F90`'s connections.
    fn connections(&mut self) -> Result<Vec<String>, Self::Error>;
    /// `00494160` with the address the dialog took.
    fn initialize_connection(&mut self, target: SearchTarget) -> Result<(), Self::Error>;
    /// `004940A0`.
    fn enumerate_sessions(&mut self) -> Result<Vec<SessionInfo>, Self::Error>;
    /// `00494190`; false when it failed (`004390A0`'s message, then
    /// `004923C0`).
    fn join(&mut self, session: &SessionInfo) -> Result<bool, Self::Error>;
    /// `00494230` then the session's start; false when it failed.
    fn host(&mut self) -> Result<bool, Self::Error>;
    /// The host's first heartbeat, which gives it its slot; whether it has
    /// one (`0055F2F4`).
    fn host_slot(&mut self) -> Result<bool, Self::Error>;
    /// The wait's receive (`0045C940`, `00492830`): this machine's slot
    /// (`0055F2F4`) and the host's world word (`0054E814`), which the receive
    /// cleared first when `clear_world` is set.
    fn wait_receive(&mut self, clear_world: bool) -> Result<(u8, i32), Self::Error>;
    /// `0045CB70`.
    fn reset_net_game(&mut self) -> Result<(), Self::Error>;
    /// `00493650`.
    fn close_session(&mut self) -> Result<(), Self::Error>;
    /// `004391B0`.
    fn play_cue(&mut self, cue: i16) -> Result<(), Self::Error>;
    fn request_phase(&mut self, phase: u32) -> Result<(), Self::Error>;
    /// `0054694C = 1`.
    fn set_transition_flag(&mut self) -> Result<(), Self::Error>;
    /// The settings DWORD at `offset` of `0054E8A8`.
    fn setting(&mut self, offset: u32) -> Result<u32, Self::Error>;
    fn set_setting(&mut self, offset: u32, value: u32) -> Result<(), Self::Error>;
    /// `00453B80` over the settings cell at `address` with `max`.
    fn adjust_setting(
        &mut self,
        address: u32,
        command: MenuCommand,
        max: u32,
    ) -> Result<(), Self::Error>;
    /// `004390A0`'s message box: the port logs it.
    fn report(&mut self, message: &str) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug)]
pub enum NetworkMenuKind {
    Connections {
        names: Vec<String>,
    },
    /// The text typed so far, and whether a Shift key is down.
    Address {
        text: String,
        shift: bool,
    },
    Sessions {
        sessions: Vec<SessionInfo>,
        refresh_micros: i32,
    },
    /// `+5C`: 0 until a slot, 1 until the host's world, 2 once phase 9 is
    /// asked for.
    Wait {
        state: u32,
    },
    /// The authored host settings `00538650`/`00538668` (vtable `005261E0`).
    Host,
}

#[derive(Debug)]
pub struct NetworkMenu {
    base: MenuState,
    kind: NetworkMenuKind,
    /// `+281`/`+5C`: whether dropping this menu closes the session; cleared
    /// when it moves forward.
    closes: bool,
    close: Option<CloseToken>,
}

/// A copy (the view shown's copy of the world's arena, menus and all) never
/// closes the session when it drops: only the menu itself does.
impl Clone for NetworkMenu {
    fn clone(&self) -> Self {
        Self {
            base: self.base.clone(),
            kind: self.kind.clone(),
            closes: false,
            close: None,
        }
    }
}

impl Drop for NetworkMenu {
    fn drop(&mut self) {
        if let (true, Some(close)) = (self.closes, &self.close) {
            close.store(true, Ordering::Relaxed);
        }
    }
}

impl NetworkMenu {
    /// `0045F9C0(false)`: the header, then a row for each connection.
    pub fn connections(
        templates: &NetworkMenuTemplates,
        layouts: MenuLayouts,
        names: Vec<String>,
    ) -> Self {
        let rows = (0..names.len() as u32).map(|index| numbered(&templates.connection, index));
        let items = std::iter::once(templates.header.clone())
            .chain(rows)
            .collect();
        let count = names.len();
        Self::list(
            CONNECTIONS_VTABLE,
            items,
            layouts,
            count,
            NetworkMenuKind::Connections { names },
        )
    }

    /// DirectPlay's TCP/IP dialog in the menu's own style: the header, then
    /// the address being typed. Blank searches the local network.
    pub fn address(templates: &NetworkMenuTemplates, layouts: MenuLayouts, text: String) -> Self {
        let items = vec![templates.header.clone(), numbered(&templates.session, 0)];
        Self::list(
            ADDRESS_VTABLE,
            items,
            layouts,
            1,
            NetworkMenuKind::Address { text, shift: false },
        )
    }

    /// `0045F9C0(true)`: the header, "Host session on this machine", then a
    /// row for each session.
    pub fn sessions(
        templates: &NetworkMenuTemplates,
        layouts: MenuLayouts,
        sessions: Vec<SessionInfo>,
    ) -> Self {
        let mut sessions = sessions;
        sessions.truncate(SESSIONS);
        let count = sessions.len() + 1;
        Self::list(
            SESSIONS_VTABLE,
            session_rows(templates, sessions.len()),
            layouts,
            count,
            NetworkMenuKind::Sessions {
                sessions,
                refresh_micros: 0,
            },
        )
    }

    /// `005386A0` with vtable `005261F4`, state 0.
    pub fn wait(data: MenuData) -> Self {
        Self::authored(data, NetworkMenuKind::Wait { state: 0 })
    }

    /// `00538650` or `00538668`, with vtable `005261E0`.
    pub fn host(data: MenuData) -> Self {
        Self::authored(data, NetworkMenuKind::Host)
    }

    fn authored(data: MenuData, kind: NetworkMenuKind) -> Self {
        Self {
            base: MenuState::new(data, MenuBasis::INITIAL),
            kind,
            closes: true,
            close: None,
        }
    }

    /// The session this menu closes when dropped.
    pub fn with_close(mut self, close: CloseToken) -> Self {
        self.close = Some(close);
        self
    }

    /// `+281 = 0` (or `+5C = 0`): moving forward keeps the session.
    pub fn keep_session(&mut self) {
        self.closes = false;
    }

    fn list(
        vtable: u32,
        items: Vec<FrontendMenuItem>,
        layouts: MenuLayouts,
        count: usize,
        kind: NetworkMenuKind,
    ) -> Self {
        size_layouts(&layouts, count);
        let data = MenuData::derived(MenuDerivedData {
            constructor: LIST_CONSTRUCTOR,
            vtable,
            items: Some(items),
            layouts: Some(layouts),
            initial_selection: 1,
            flags: 0,
        });
        Self::authored(data, kind)
    }

    pub fn base(&self) -> &MenuState {
        &self.base
    }
    pub fn base_mut(&mut self) -> &mut MenuState {
        &mut self.base
    }
    pub fn kind(&self) -> &NetworkMenuKind {
        &self.kind
    }

    /// `0045F700`'s text for the row whose argument is `index`.
    pub fn row_text(&self, index: u32) -> String {
        let name = match &self.kind {
            NetworkMenuKind::Connections { names } => names.get(index as usize).cloned(),
            NetworkMenuKind::Sessions { sessions, .. } => sessions
                .get(index as usize)
                .map(|session| session.name.clone()),
            NetworkMenuKind::Address { text, .. } => return format!("{ADDRESS_LABEL}{text}_"),
            NetworkMenuKind::Wait { .. } | NetworkMenuKind::Host => None,
        };
        name.filter(|name| !name.is_empty())
            .map(|name| name.chars().take(NAME_BYTES).collect())
            .unwrap_or_else(|| NO_MORE.to_string())
    }

    pub fn is_address_prompt(&self) -> bool {
        matches!(self.kind, NetworkMenuKind::Address { .. })
    }

    /// A key notification while the address prompt shows: Shift is tracked
    /// and passed on; a typed key is taken (true) and edits the text.
    pub fn key(&mut self, scancode: &str, pressed: bool) -> bool {
        let NetworkMenuKind::Address { shift, .. } = &mut self.kind else {
            return false;
        };
        if matches!(scancode, "Left Shift" | "Right Shift") {
            *shift = pressed;
            return false;
        }
        let Some(key) = typed_key(scancode, *shift) else {
            return false;
        };
        if pressed {
            self.type_key(key);
        }
        true
    }

    /// A typed key while the address prompt shows.
    pub fn type_key(&mut self, key: TypedKey) {
        if let NetworkMenuKind::Address { text, .. } = &mut self.kind {
            match key {
                TypedKey::Char(character) if text.len() < NAME_BYTES => text.push(character),
                TypedKey::Char(_) => {}
                TypedKey::Backspace => {
                    text.pop();
                }
            }
        }
    }

    /// The tick: `0045FDE0` searches the sessions again once a second, and
    /// rebuilds the rows when their count changed; `0045FF80` runs the wait.
    /// Then the base tick (`00452470`).
    pub fn tick<S: NetworkMenuServices>(
        &mut self,
        delta_micros: i32,
        templates: &NetworkMenuTemplates,
        services: &mut S,
    ) -> Result<(), NetworkMenuError<S::Error>> {
        match &mut self.kind {
            NetworkMenuKind::Sessions {
                sessions,
                refresh_micros,
            } => {
                *refresh_micros = refresh_micros.wrapping_add(delta_micros);
                if *refresh_micros > REFRESH_MICROS {
                    *refresh_micros = 0;
                    let before = sessions.len();
                    *sessions = services
                        .enumerate_sessions()
                        .map_err(NetworkMenuError::Host)?;
                    sessions.truncate(SESSIONS);
                    if sessions.len() != before {
                        let count = sessions.len() + 1;
                        let data = self.base.data().clone();
                        {
                            let mut records = data.records_mut();
                            *records.items = session_rows(templates, sessions.len());
                        }
                        size_layouts(&data.layouts(), count);
                        // A selection past the rebuilt rows would read past
                        // retail's array; the port keeps it on the last row.
                        let selection = self.base.selection().min(count as i32);
                        let scroll = self.base.scroll_start();
                        self.base.replace_data(data, selection, Some(scroll));
                    }
                }
            }
            NetworkMenuKind::Wait { state } => {
                if *state == 0 {
                    let (slot, _) = services
                        .wait_receive(false)
                        .map_err(NetworkMenuError::Host)?;
                    if slot != 0 {
                        *state = 1;
                        services.reset_net_game().map_err(NetworkMenuError::Host)?;
                    }
                } else if *state == 1 {
                    let (_, world) = services
                        .wait_receive(true)
                        .map_err(NetworkMenuError::Host)?;
                    if world != 0 {
                        // 0045FFB6: the session (004391B0) asks for phase 9.
                        *state = 2;
                        self.closes = false;
                        services.request_phase(9).map_err(NetworkMenuError::Host)?;
                    }
                }
            }
            NetworkMenuKind::Connections { .. }
            | NetworkMenuKind::Address { .. }
            | NetworkMenuKind::Host => {}
        }
        self.base
            .tick_with_rotation(delta_micros, |basis, angle| {
                services.rotate_menu_basis(basis, angle)
            })
            .map_err(NetworkMenuError::Host)
    }
}

/// A typed key for the address prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypedKey {
    Char(char),
    Backspace,
}

/// The address prompt's keys, by SDL scancode name: letters, digits, the
/// period, the minus, Shift and the semicolon for the port's colon, and
/// Backspace.
pub fn typed_key(scancode: &str, shift: bool) -> Option<TypedKey> {
    let key = scancode.strip_prefix("Keypad ").unwrap_or(scancode);
    let character = match key {
        "Backspace" => return Some(TypedKey::Backspace),
        "Period" | "." => '.',
        "Minus" | "-" => '-',
        "Semicolon" | ";" if shift => ':',
        _ => {
            let mut chars = key.chars();
            let first = chars.next()?;
            if chars.next().is_some() || !first.is_ascii_alphanumeric() {
                return None;
            }
            if shift {
                first
            } else {
                first.to_ascii_lowercase()
            }
        }
    };
    Some(TypedKey::Char(character))
}

fn numbered(template: &FrontendMenuItem, index: u32) -> FrontendMenuItem {
    let mut row = template.clone();
    row.draw_arguments[0] = index;
    row.action_arguments[0] = index;
    row
}

fn session_rows(templates: &NetworkMenuTemplates, sessions: usize) -> Vec<FrontendMenuItem> {
    [templates.header.clone(), templates.host.clone()]
        .into_iter()
        .chain((0..sessions as u32).map(|index| numbered(&templates.session, index)))
        .collect()
}

/// `005384C1`/`C2`: the second layout counts the rows under the header, and
/// shows at most four.
fn size_layouts(layouts: &MenuLayouts, count: usize) {
    if let Some(rows) = layouts.records_mut().get_mut(1) {
        rows.item_count = count as u8;
        rows.visible_rows = (count as u8).min(VISIBLE_ROWS);
    }
}

/// `0045F740`, a list row's Activate (the only command it takes).
pub fn list_action<S: NetworkMenuServices>(
    stack: &mut MenuStack,
    request: MenuActionRequest,
    templates: &NetworkMenuTemplates,
    services: &mut S,
) -> Result<(), NetworkMenuError<S::Error>> {
    if request.command != MenuCommand::Activate {
        return Ok(());
    }
    let index = request.arguments[0];
    let layouts = |services: &mut S| -> Result<MenuLayouts, NetworkMenuError<S::Error>> {
        Ok(services
            .menu_data(HOST_MENU)
            .map_err(NetworkMenuError::Host)?
            .layouts())
    };
    let close = services.close_token().map_err(NetworkMenuError::Host)?;
    let Some(MenuOwner::Network(menu)) = stack.current_owner() else {
        return Ok(());
    };
    match menu.kind() {
        NetworkMenuKind::Connections { .. } => {
            // 00494160 waits for the dialog's address here.
            let layouts = layouts(services)?;
            keep_current(stack);
            stack
                .replace(NetworkMenu::address(templates, layouts, String::new()).with_close(close));
        }
        NetworkMenuKind::Address { text, .. } => match SearchTarget::parse(text) {
            Ok(target) => {
                services
                    .initialize_connection(target)
                    .map_err(NetworkMenuError::Host)?;
                let sessions = services
                    .enumerate_sessions()
                    .map_err(NetworkMenuError::Host)?;
                let layouts = layouts(services)?;
                keep_current(stack);
                stack
                    .replace(NetworkMenu::sessions(templates, layouts, sessions).with_close(close));
            }
            Err(error) => {
                services
                    .report(&error.to_string())
                    .map_err(NetworkMenuError::Host)?;
                services.play_cue(4).map_err(NetworkMenuError::Host)?;
            }
        },
        NetworkMenuKind::Sessions { sessions, .. } => {
            if index == u32::MAX {
                // 0045F79B: the host menu's own row counts, then 00538650.
                let data = services
                    .menu_data(HOST_MENU)
                    .map_err(NetworkMenuError::Host)?;
                if let Some(rows) = data.layouts().records_mut().get_mut(1) {
                    rows.item_count = HOST_ROWS.0;
                    rows.visible_rows = HOST_ROWS.1;
                }
                keep_current(stack);
                stack.replace(NetworkMenu::host(data).with_close(close));
                show_mode_rows(stack, services)?;
                return Ok(());
            }
            let session = sessions
                .get(index as usize)
                .cloned()
                .ok_or(NetworkMenuError::Row(index))?;
            if !services.join(&session).map_err(NetworkMenuError::Host)? {
                return Ok(());
            }
            services
                .set_transition_flag()
                .map_err(NetworkMenuError::Host)?;
            let data = services
                .menu_data(WAIT_MENU)
                .map_err(NetworkMenuError::Host)?;
            keep_current(stack);
            stack.replace(NetworkMenu::wait(data).with_close(close));
        }
        NetworkMenuKind::Wait { .. } | NetworkMenuKind::Host => {}
    }
    Ok(())
}

fn keep_current(stack: &mut MenuStack) {
    if let Some(MenuOwner::Network(menu)) = stack.current_owner_mut() {
        menu.keep_session();
    }
}

/// `0045F670`: the rows the arena's mode shows. Modes 0 to 2 show
/// Recharging grenades and the Winning Score; 3 (the races) Catchup Scale,
/// and sets Recharging grenades; 4 (Powerball) the Winning Score.
pub fn show_mode_rows<S: NetworkMenuServices>(
    stack: &mut MenuStack,
    services: &mut S,
) -> Result<(), NetworkMenuError<S::Error>> {
    let arena = services
        .setting(SETTING_ARENA)
        .map_err(NetworkMenuError::Host)?;
    let mode = arena_at(arena).mode;
    let shown = |on: bool| if on { ROW_SHOWN } else { 0 };
    let flags = match mode {
        0..=2 => [shown(true), shown(false), shown(true)],
        3 => {
            services
                .set_setting(SETTING_GRENADES, 1)
                .map_err(NetworkMenuError::Host)?;
            [shown(false), shown(true), shown(false)]
        }
        4 => [shown(false), shown(false), shown(true)],
        _ => return Ok(()),
    };
    if let Some(menu) = stack.current() {
        let data = menu.data().clone();
        let mut records = data.records_mut();
        for (row, flag) in [GRENADES_ROW, CATCHUP_ROW, SCORE_ROW]
            .into_iter()
            .zip(flags)
        {
            if let Some(item) = records.items.get_mut(row) {
                item.flags = flag;
            }
        }
    }
    Ok(())
}

/// The host menu's callbacks. Activate on any of its setting rows starts the
/// session, and Start Session's `0045F320` takes no command, so every
/// command that reaches it does.
pub fn host_menu_action<S: NetworkMenuServices>(
    stack: &mut MenuStack,
    request: MenuActionRequest,
    services: &mut S,
) -> Result<(), NetworkMenuError<S::Error>> {
    let command = request.command;
    let [max, cell] = request.arguments;
    if request.callback_address == BACK_ACTION {
        // 00453C50 pops the menu, whose destructor closes the session.
        if command == MenuCommand::Activate {
            stack.pop(1);
        }
        return Ok(());
    }
    if command == MenuCommand::Activate || request.callback_address == START_ACTION {
        return start_session(stack, services);
    }
    match request.callback_address {
        ARENA_ACTION => {
            // 0045F579: eleven arenas with the cheat word's bit 7, else ten.
            let cheats = services
                .setting(SETTING_CHEATS)
                .map_err(NetworkMenuError::Host)?;
            let max = if cheats & 0x80 != 0 { max } else { 9 };
            services
                .adjust_setting(cell, command, max)
                .map_err(NetworkMenuError::Host)?;
            let data = services
                .menu_data(HOST_MENU_RELAID)
                .map_err(NetworkMenuError::Host)?;
            let close = services.close_token().map_err(NetworkMenuError::Host)?;
            keep_current(stack);
            stack.replace(NetworkMenu::host(data).with_close(close));
            show_mode_rows(stack, services)?;
        }
        VALUE_ACTION => services
            .adjust_setting(cell, command, max)
            .map_err(NetworkMenuError::Host)?,
        LIMIT_ACTION => {
            let arena = services
                .setting(SETTING_ARENA)
                .map_err(NetworkMenuError::Host)?;
            let step = if matches!(arena_at(arena).mode, 3 | 4) {
                1000
            } else {
                500
            };
            let value = services
                .setting(SETTING_WINNING_SCORE)
                .map_err(NetworkMenuError::Host)? as i32;
            let next = match command {
                MenuCommand::AdjustPrevious => value.wrapping_sub(step).max(1000),
                MenuCommand::AdjustNext => value.wrapping_add(step).min(max as i32),
                _ => return Ok(()),
            };
            services
                .set_setting(SETTING_WINNING_SCORE, next as u32)
                .map_err(NetworkMenuError::Host)?;
        }
        _ => {}
    }
    Ok(())
}

/// `0045F320`: host the session, record the arena's mode in settings `+10`,
/// and once the host has its slot, ask for phase 9. Without one the session
/// closes and the menu pops.
fn start_session<S: NetworkMenuServices>(
    stack: &mut MenuStack,
    services: &mut S,
) -> Result<(), NetworkMenuError<S::Error>> {
    if !services.host().map_err(NetworkMenuError::Host)? {
        return Ok(());
    }
    let arena = services
        .setting(SETTING_ARENA)
        .map_err(NetworkMenuError::Host)?;
    services
        .set_setting(SETTING_MODE, arena_at(arena).mode)
        .map_err(NetworkMenuError::Host)?;
    services
        .set_transition_flag()
        .map_err(NetworkMenuError::Host)?;
    services.reset_net_game().map_err(NetworkMenuError::Host)?;
    if services.host_slot().map_err(NetworkMenuError::Host)? {
        if !services.network_game_ready() {
            services
                .report("the session is up, but the network game itself is not ported yet")
                .map_err(NetworkMenuError::Host)?;
            services.close_session().map_err(NetworkMenuError::Host)?;
            return Ok(());
        }
        keep_current(stack);
        services.request_phase(9).map_err(NetworkMenuError::Host)?;
        return Ok(());
    }
    services
        .report("the session gave this machine no slot")
        .map_err(NetworkMenuError::Host)?;
    services.close_session().map_err(NetworkMenuError::Host)?;
    stack.pop(1);
    Ok(())
}

/// `0045F280`: the Winning Score's number, in thousands in mode 4.
pub fn winning_score_text(mode: u32, value: u32) -> String {
    let shown = if mode == 4 {
        value as i32 / 1000
    } else {
        value as i32
    };
    format!("{shown}")
}

#[cfg(test)]
#[path = "network_menu_tests.rs"]
mod tests;
