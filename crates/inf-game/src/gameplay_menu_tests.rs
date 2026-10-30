use super::{
    deselect_effects, f8_latch, map_requested, menu_key_effects, return_to_frontend_requested,
    DeselectEffects, MenuKeyTerminal, WORLD_COMPLETE_SELECTOR, WORLD_COMPLETE_STATUS,
};
use crate::session::SessionPhase;

#[test]
fn attract_path_requests_frontend_and_still_reaches_terminal() {
    let effects = menu_key_effects(true, 0, false);
    assert!(effects.clear_attract);
    assert_eq!(effects.frontend_request, Some(SessionPhase::Frontend));
    assert_eq!(effects.terminal, MenuKeyTerminal::RequestMenu);
}

#[test]
fn attract_path_with_complete_world_still_takes_world_update() {
    let effects = menu_key_effects(true, WORLD_COMPLETE_STATUS, true);
    assert_eq!(effects.frontend_request, Some(SessionPhase::Frontend));
    assert_eq!(
        effects.terminal,
        MenuKeyTerminal::WorldUpdate {
            selector: WORLD_COMPLETE_SELECTOR
        }
    );
}

#[test]
fn complete_world_calls_world_update_without_frontend_request() {
    let effects = menu_key_effects(false, WORLD_COMPLETE_STATUS, false);
    assert!(!effects.clear_attract);
    assert_eq!(effects.frontend_request, None);
    assert_eq!(
        effects.terminal,
        MenuKeyTerminal::WorldUpdate { selector: 0x12 }
    );
}

#[test]
fn live_player_modal_takes_player_branch() {
    let effects = menu_key_effects(false, 0, true);
    assert_eq!(effects.terminal, MenuKeyTerminal::PlayerModal);
}

#[test]
fn plain_keypress_requests_phase_5() {
    let effects = menu_key_effects(false, 0, false);
    assert_eq!(effects.frontend_request, None);
    assert_eq!(effects.terminal, MenuKeyTerminal::RequestMenu);
    assert_eq!(super::MENU_PHASE, SessionPhase::Phase5);
}

#[test]
fn return_to_frontend_fires_only_on_nonzero_argument() {
    assert_eq!(return_to_frontend_requested(0), None);
    assert_eq!(
        return_to_frontend_requested(1),
        Some(SessionPhase::Frontend)
    );
}

#[test]
fn map_request_needs_nonzero_argument_incomplete_offline_world() {
    assert_eq!(map_requested(0, 0, false), None);
    assert_eq!(map_requested(0x21, WORLD_COMPLETE_STATUS, false), None);
    assert_eq!(map_requested(0x21, 0, true), None);
    assert_eq!(map_requested(0x21, 0, false), Some(SessionPhase::Phase7));
}

#[test]
fn f8_latch_reflects_nonzero_argument() {
    assert!(!f8_latch(0));
    assert!(f8_latch(1));
}

#[test]
fn deselect_returns_early_on_network_complete_or_attract() {
    assert_eq!(deselect_effects(true, 0, false), None);
    assert_eq!(deselect_effects(false, WORLD_COMPLETE_STATUS, false), None);
    assert_eq!(deselect_effects(false, 0, true), None);
}

#[test]
fn deselect_runs_prepare_step_and_requests_phase_5() {
    assert_eq!(
        deselect_effects(false, 0, false),
        Some(DeselectEffects {
            prepare_step: true,
            menu_request: Some(SessionPhase::Phase5),
        })
    );
}
