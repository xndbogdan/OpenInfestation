//! The phase-7 map screen's input and tick.

use super::intro::{deliver, flatten_registry, require_id, system_callback};
use super::*;
use crate::map_screen::{MapInputAction, MapScreen};

impl PortLinked<'_> {
    pub fn poll_map(
        &mut self,
        state: &mut SessionState,
        pass: PortInputPass<'_>,
        exit_requested: &mut bool,
    ) -> Result<(), GameplayLinkedError> {
        let owner = self.attachment.context_owner;
        let screen = self.map.as_deref_mut().ok_or(LinkedScreenError::Unlinked)?;
        let input = &mut *self.input;
        let result = self.registry.with_context_dispatch(owner, |registry| {
            require_id(registry, owner, screen.id())?;
            let now_micros = pass.now_micros;
            deliver(
                pass,
                input,
                &mut MapRouter {
                    now_micros,
                    screen,
                    state,
                    exit_requested,
                },
            )
        });
        flatten_registry(result)
    }

    /// Tick `00451F30`.
    pub fn tick_map(&mut self, tick: SchedulerTick) -> Result<(), GameplayLinkedError> {
        let owner = self.attachment.context_owner;
        let screen = self.map.as_deref_mut().ok_or(LinkedScreenError::Unlinked)?;
        let input = &*self.input;
        let result = self.registry.with_context_dispatch(owner, |registry| {
            require_id(registry, owner, screen.id())?;
            screen
                .tick(tick, input)
                .map_err(|error| GameplayLinkedError::Tick(error.to_string()))
        });
        flatten_registry(result)
    }
}

/// System chords behave as on every screen; the map's own callbacks reach
/// the screen, whose context check rejects any other owner's.
struct MapRouter<'a> {
    now_micros: u64,
    screen: &'a mut MapScreen,
    state: &'a mut SessionState,
    exit_requested: &'a mut bool,
}

impl InputHost for MapRouter<'_> {
    type Error = GameplayLinkedError;
    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(self.now_micros)
    }
    fn callback(&mut self, event: InputCallback, _: &mut InputRuntime) -> Result<(), Self::Error> {
        if system_callback(event, self.exit_requested) {
            return Ok(());
        }
        match self
            .screen
            .digital_callback(event)
            .map_err(|error| GameplayLinkedError::Tick(error.to_string()))?
        {
            MapInputAction::RequestGameplay => {
                let _ = self.state.request_phase(SessionPhase::Gameplay);
            }
            MapInputAction::None => {}
        }
        Ok(())
    }
}
