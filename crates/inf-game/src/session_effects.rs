//! Session transition effects over complete caller-owned phases.
//!
//! Preparation owns successful player allocation, input/grant construction and
//! mission reset in native order; it cannot publish only a saved-player template.
//! LeaveWorld retains the same manager, constructor carriers and acquiring input
//! owner through reached cleanup and deletion. Attract/network phase admission
//! belongs to the application; these phases keep their own lazy source reads.

use thiserror::Error;

use crate::session::{SessionState, TransitionEffect};

#[derive(Debug, Error)]
pub enum SessionEffectsError<E> {
    #[error("session effect host failed")]
    Host(E),
    #[error("session effect {0:?} requires the gameplay/world backend")]
    Unsupported(TransitionEffect),
}

/// Every operation is required; no successful default exists.
pub trait SessionEffectsHost {
    type Error;
    /// Complete admitted 00487410 over the retained actual player owners.
    /// The session elapsed alias is updated only after reached mission reset.
    fn prepare_offline_game(&mut self, state: &mut SessionState) -> Result<(), Self::Error>;
    /// Complete admitted 004871C0. Reached failures retain actual custody;
    /// no saved-state clear or Rust drop substitutes for player destruction.
    /// Births a destructor reaches read the session's current phase.
    fn clear_prepared_world(&mut self, state: &SessionState) -> Result<(), Self::Error>;
    /// 00487360 frontend music selection.
    fn select_frontend_music(&mut self) -> Result<(), Self::Error>;
    /// 004F0340 baseline reset and queue discard.
    fn reset_input_baselines(&mut self) -> Result<(), Self::Error>;
}

/// Dispatch a complete phase without moving its lower reads ahead of it.
pub fn apply_transition_effect<H: SessionEffectsHost>(
    effect: TransitionEffect,
    host: &mut H,
    state: &mut SessionState,
) -> Result<(), SessionEffectsError<H::Error>> {
    match effect {
        TransitionEffect::PreparePhase9 => host
            .prepare_offline_game(state)
            .map_err(SessionEffectsError::Host),
        TransitionEffect::LeaveWorld => host
            .clear_prepared_world(state)
            .map_err(SessionEffectsError::Host),
        TransitionEffect::SelectFrontendMusic => host
            .select_frontend_music()
            .map_err(SessionEffectsError::Host),
        TransitionEffect::ResetInputBaselines => host
            .reset_input_baselines()
            .map_err(SessionEffectsError::Host),
        TransitionEffect::PrepareFromFrontend
        | TransitionEffect::LoadWorld
        | TransitionEffect::SelectWorldMusic
        | TransitionEffect::ResumeWorldMusic
        | TransitionEffect::PrepareResults
        | TransitionEffect::SetPlayerTimer { .. } => Err(SessionEffectsError::Unsupported(effect)),
    }
}

#[cfg(test)]
#[path = "session_effects_tests.rs"]
mod tests;
