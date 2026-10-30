//! Gameplay screen menu/attract decision gates: `00443070` (`+0x64`),
//! `00442FD0` (`+0x54`), `00442BC0` (`+0x40`), `00440940` (`+0x58`) and
//! deselect `00440BA0` (`+0x14`).
//!
//! Native behavior, recovered from the supported retail `Infest.exe` and
//! bounded TTD replay (see the live sample and the virtual-dispatch table in
//! [gameplay runtime](../../../docs/re/GAMEPLAY_RUNTIME.md#screen-tick-detail)):
//!
//! - `00443070` runs on the Escape-class key path: when the attract flag
//!   (`0055F14C`) is set it clears the flag and requests phase 2, then
//!   **falls through** (no return); when the world-object field `+0x250`
//!   reads `0x16` it calls `00487560(0x12,1,0)`; on a live player modal
//!   (`00488AA0` nonzero with byte `+0x88` set) it calls `004133C0`;
//!   otherwise it requests phase 5 through `00438380`.
//! - `00442FD0` requests phase 2 when its argument is nonzero; it is shared
//!   by the gameplay (`00523370`) and phase-5 (`005233D8`) vtables.
//! - `00442BC0` requests phase 7 when its argument is nonzero, unless the
//!   world-object field `+0x250` reads `0x16` or the network flag
//!   (`0055F448`) is set.
//! - `00440940` latches screen `+F8` to `(argument != 0)`; it is shared by
//!   both vtables.
//! - Deselect `00440BA0` returns early with the network flag set, the
//!   world-object field `+0x250` equal to `0x16`, or the attract flag set;
//!   otherwise it runs `00487230` and requests phase 5 (the request result
//!   is discarded).
//! - The inner tick worker `00440D80` forwards both scheduler timing
//!   arguments to base input `004392B0` with no decision of its own; the
//!   gameplay tick tail reaches it after latching settings `+8`.
//!
//! These are pure decision gates over explicit inputs. Heap reads (the
//! attract byte, the `+0x250` field, the player lookup, the network flag),
//! the `00487560`/`004133C0`/`00487230` calls and the `00438380`
//! first-request-wins submission stay with the future world/session host;
//! this owner only reports which effects the native code selects.

use crate::session::SessionPhase;

/// World-object `+0x250` value diverting the menu/map paths to the
/// results/update call instead of a phase request.
pub const WORLD_COMPLETE_STATUS: u32 = 0x16;

/// `00487560` first argument on the `00443070` world-complete path.
pub const WORLD_COMPLETE_SELECTOR: u32 = 0x12;

/// Terminal branch of `00443070` after the attract fall-through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKeyTerminal {
    /// Call `00487560(selector, 1, 0)`; the selector is always
    /// [`WORLD_COMPLETE_SELECTOR`] on this path.
    WorldUpdate { selector: u32 },
    /// Call `004133C0` on the live player modal.
    PlayerModal,
    /// Request phase 5 through `00438380`.
    RequestMenu,
}

/// Selected effects of `00443070` for one key event.
///
/// `frontend_request` is `Some` exactly when the attract flag entered set;
/// the native code still falls through to `terminal` afterwards instead of
/// returning, which this shape preserves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuKeyEffects {
    pub clear_attract: bool,
    pub frontend_request: Option<SessionPhase>,
    pub terminal: MenuKeyTerminal,
}

/// `00443070`: attract fall-through, then the world-complete / player-modal
/// / menu-request terminal branch.
pub fn menu_key_effects(attract: bool, world_status: u32, player_modal: bool) -> MenuKeyEffects {
    let terminal = if world_status == WORLD_COMPLETE_STATUS {
        MenuKeyTerminal::WorldUpdate {
            selector: WORLD_COMPLETE_SELECTOR,
        }
    } else if player_modal {
        MenuKeyTerminal::PlayerModal
    } else {
        MenuKeyTerminal::RequestMenu
    };
    MenuKeyEffects {
        clear_attract: attract,
        frontend_request: attract.then_some(SessionPhase::Frontend),
        terminal,
    }
}

/// The phase `00443070` requests on its [`MenuKeyTerminal::RequestMenu`]
/// branch.
pub const MENU_PHASE: SessionPhase = SessionPhase::Phase5;

/// `00442FD0`: nonzero argument requests phase 2; zero returns.
pub fn return_to_frontend_requested(argument: u32) -> Option<SessionPhase> {
    (argument != 0).then_some(SessionPhase::Frontend)
}

/// `00442BC0`: nonzero argument requests phase 7 unless the world is
/// complete or a network session is active; zero returns.
pub fn map_requested(
    argument: u32,
    world_status: u32,
    network_active: bool,
) -> Option<SessionPhase> {
    if argument != 0 && world_status != WORLD_COMPLETE_STATUS && !network_active {
        Some(SessionPhase::Phase7)
    } else {
        None
    }
}

/// `00440940`: screen `+F8` latch value for the argument.
pub fn f8_latch(argument: u32) -> bool {
    argument != 0
}

/// Selected effects of deselect `00440BA0`: `None` is an early return;
/// `Some` runs `00487230` and requests phase 5 (the request result is
/// discarded by the native code).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeselectEffects {
    pub prepare_step: bool,
    pub menu_request: Option<SessionPhase>,
}

/// `00440BA0`: early return with a network session, a complete world, or
/// attract set; otherwise the prepare step plus the phase-5 request.
pub fn deselect_effects(
    network_active: bool,
    world_status: u32,
    attract: bool,
) -> Option<DeselectEffects> {
    if network_active || world_status == WORLD_COMPLETE_STATUS || attract {
        None
    } else {
        Some(DeselectEffects {
            prepare_step: true,
            menu_request: Some(SessionPhase::Phase5),
        })
    }
}

#[cfg(test)]
#[path = "gameplay_menu_tests.rs"]
mod tests;
