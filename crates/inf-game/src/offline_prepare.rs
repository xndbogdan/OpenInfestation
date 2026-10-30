//! Saved-data preparation for standalone offline/import fixtures.
//!
//! The offline 00487410 branch pushes selection 0x16 (22) and `true` into
//! `0048EBA0(owner + 0x1B0, 22, true)`, resetting the serialized mission
//! state to the offline defaults. It sets gameplay owner byte `+0x5C2` to 1
//! and constructs a live 0x1A0-byte player through 00469D60 before clearing
//! armed byte `+0x5D8` and resetting the mission. These helpers project mission
//! and initial grant-mask fields into saved data; they do not construct a
//! live player. Actual offline input/player/armed/reset ownership is in
//! player_prepare and session_platform::world_owners. Native allocation-NULL
//! handling and production attract/network admission remain boundaries.

use thiserror::Error;

use inf_formats::{
    grant_closure, GrantClosureError, GrantTable, MissionSaveState, PlayerSaveState,
    PLAYER_INITIAL_GRANTS,
};

/// Offline mission selection pushed by 00487410 (`push 0x16`).
pub const OFFLINE_MISSION_SELECTION: u32 = 22;

/// The serialized state 00487410 leaves behind for offline 2->9 preparation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfflineGame {
    pub mission: MissionSaveState,
    pub player: PlayerSaveState,
}

/// 0048EBA0(22, true) plus the serialized half of 00469D60: the twelve
/// 00413140 construction grants closed over the parsed dependency table.
pub fn offline_game(table: &GrantTable) -> Result<OfflineGame, OfflineGameError> {
    offline_game_with_selection(table, None)
}

/// Debug-only variant: `None` keeps the retail 22 push; `Some` selects
/// another mission's world for the offline session (used by `--mission`
/// acceptance runs; rendering and simulation fidelity are unchanged —
/// only the selected OVL differs).
pub fn offline_game_with_selection(
    table: &GrantTable,
    mission: Option<u32>,
) -> Result<OfflineGame, OfflineGameError> {
    let mut mission_state = MissionSaveState::new_game();
    if let Some(selection) = mission {
        mission_state.mission_selection = selection;
    }
    debug_assert_eq!(
        mission_state.mission_selection,
        mission.unwrap_or(OFFLINE_MISSION_SELECTION)
    );
    let mask = grant_closure(table, &PLAYER_INITIAL_GRANTS)?;
    Ok(OfflineGame {
        mission: mission_state,
        player: PlayerSaveState::new_player(mask),
    })
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OfflineGameError {
    #[error(transparent)]
    Grant(#[from] GrantClosureError),
}

#[cfg(test)]
#[path = "offline_prepare_tests.rs"]
mod tests;
