//! Intro-only phase-8 factory over actual menu, input, player and process owners.
//! Serialize/Restore borrow the retained working profile after input selection.
//! The final world's name read is phase 9's (00463550).

use super::*;
use crate::frontend_bindings::FrontendDevice;
use crate::frontend_screen::FrontendFactoryHost;
use crate::intro_results_screen::{
    IntroResultsFactoryHost, IntroResultsFactoryServices, IntroResultsProfileOperation,
    IntroResultsScreenConfig, IntroResultsSkippedFeature, INTRO_RESULTS_CALLBACK_CONTEXT,
};

pub(crate) mod retirement;
mod runtime;
pub(in crate::session_platform) use runtime::{
    request_player_message_phases, sync_player_message_ui_state,
};

#[cfg(test)]
pub(crate) mod required_miss_observer;

/// The platform, and 0055F2F4 for the final world's name read.
struct IntroResultsFactoryAdapter<'a, P>(&'a mut P, u8);

impl<P: PortPlatform> IntroResultsFactoryHost for IntroResultsFactoryAdapter<'_, P> {
    type Error = <P as FrontendFactoryHost>::Error;
    type IdentityError = <P as PortSessionServices>::IdentityError;
    fn reset_loading_mode_42ac48(&mut self) {
        self.0.loading_platform().mode.set(0);
    }
    fn persist_identity(
        &mut self,
        request: crate::name_entry::IdentityWriteRequest<'_>,
    ) -> Result<(), Self::IdentityError> {
        PortSessionServices::persist_identity(self.0, request)
    }
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        FrontendFactoryHost::enumerate_calibrated_devices(self.0, input)
    }
    fn profile_store(&mut self) -> Option<&mut crate::profile_store::ProfileStore> {
        PortSessionServices::profile_store(self.0)
    }
    fn report_profile_failure(
        &mut self,
        operation: IntroResultsProfileOperation,
        error: &dyn std::error::Error,
    ) {
        eprintln!("Results screen: working-profile {operation:?} failed: {error}");
    }
    fn skip_feature(&mut self, feature: IntroResultsSkippedFeature) -> Result<(), Self::Error> {
        eprintln!("Results screen: skipped {feature:?}; its native owner is not ported");
        Ok(())
    }
    fn player_name(&mut self) -> Result<Vec<u8>, String> {
        crate::text_entry_screen::TextEntryFactoryHost::read_identity(
            self.0,
            SessionPhase::Phase9,
            self.1,
        )
        .map(|identity| identity.name)
        .map_err(|_| "00463550's identity read failed".into())
    }
}

impl<P: PortPlatform> PortSession<P> {
    /// Validate the admitted load and bridge the one actual mission timer.
    /// This does not export/reimport a live player or replace mission backing.
    pub(super) fn retain_offline_world_load(
        &mut self,
        state: &SessionState,
    ) -> Result<(), PortSessionErrorOf<P>> {
        if self.offline_game.is_none() {
            return Err(PortSessionError::Boundary(SessionBoundary::Phase {
                previous: state.current_phase(),
                requested: state.requested_phase(),
            }));
        }
        if self.player_prepare_stopped.is_some() {
            return Err(PortSessionError::WorldOwnerBoundary(
                "stopped player preparation cannot enter LoadWorld",
            ));
        }
        if state.current_phase() == SessionPhase::Phase8 {
            let world = self
                .world
                .as_mut()
                .ok_or(PortSessionError::World(WorldBackendError::MissingWorld))?;
            // Results/briefing do not advance the retained gameplay remainder.
            world.mission.save.elapsed_seconds = state.gameplay_time().seconds;
        }
        Ok(())
    }

    pub(super) fn prepare_intro_results(
        &mut self,
        state: &SessionState,
    ) -> Result<(), PortSessionErrorOf<P>> {
        // 00487540 zeroes G+5B4 before 0048E890, so the retirement's kind-7
        // pickup destructors count only the pickups left in the world.
        if let Some(world) = self.world.as_mut() {
            world.arena.reset_pickup_kind7_count();
        }
        //487540→48E890 retires the map before byte12/effect cleanup, while
        // the player/mission save carrier stays owned by this session.
        self.retire_world_map()?;
        let world = self
            .world
            .as_mut()
            .ok_or(PortSessionError::World(WorldBackendError::MissingWorld))?;
        // 0048E890 clears W+12 before invoking player/world cleanup callbacks.
        world.mission.byte_12 = Some(0);
        self.clear_world_effects();
        let world = self.world.as_mut().expect("retained results world");
        if matches!(
            world.owner_retirement,
            crate::world_backend::WorldOwnerRetirement::NotStarted
        ) {
            let retirement = self.retire_loaded_world_owners(state);
            let world = self.world.as_mut().expect("retained results world");
            world.owner_retirement = match retirement {
                Ok(()) => crate::world_backend::WorldOwnerRetirement::Completed,
                Err(error) => {
                    eprintln!("World retirement feature stopped after reached writes: {error}");
                    crate::world_backend::WorldOwnerRetirement::Stopped(error)
                }
            };
        }
        Ok(())
    }

    pub(super) fn construct_intro_results(
        &mut self,
        state: &mut SessionState,
    ) -> Result<(), PortSessionErrorOf<P>> {
        if self.intro_results.is_some() {
            return Err(PortSessionError::ScreenAlreadyPresent(
                PortScreen::IntroResults,
            ));
        }
        let world = self
            .world
            .as_mut()
            .ok_or(PortSessionError::World(WorldBackendError::MissingWorld))?;
        let templates = self
            .catalog
            .profile_templates()
            .map_err(|error| PortSessionError::IntroResults(error.to_string()))?;
        let assets = self.catalog.assets();
        let mut screen = IntroResultsScreen::new(
            IntroResultsScreenConfig {
                attachment: self.attachment,
                callback_context: INTRO_RESULTS_CALLBACK_CONTEXT,
            },
            assets,
        );
        let result = screen.initialize(
            &mut IntroResultsFactoryServices {
                registry: &mut self.registry,
                input: &mut self.input,
                assets,
                arena: &mut world.arena,
                mission: &mut world.mission,
                process: Some(&self.factory_process),
                classification: &self.profile_name_classification,
                profile_templates: &templates,
            },
            &mut IntroResultsFactoryAdapter(&mut self.platform, self.factory_process.local_slot),
            state,
        );
        self.intro_results = Some(screen);
        result.map_err(|error| PortSessionError::IntroResults(error.to_string()))
    }
}
