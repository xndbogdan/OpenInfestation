//! Session custody of the phase-7 map screen.
//!
//! The computer's "Map and Objectives" (`00415170`) requests phase 7 from
//! gameplay; the gameplay screen's reuse rule (`00440B60`) refuses it, so
//! the transition destroys it and `00450FC0` builds the map. The map's
//! `004520A0` requests phase 4, whose screen is built afresh. The session
//! tick runs the player callback (`00438950`) meanwhile, not the world.

use super::{PortPlatform, PortScreen, PortSession, PortSessionError, PortSessionErrorOf};
use crate::frontend_screen::FrontendFactoryHost;
use crate::input::InputRuntime;
use crate::map_screen::{
    MapFactoryHost, MapFactoryServices, MapScreen, MapScreenConfig, MAP_CALLBACK_CONTEXT,
};
use crate::session::SessionState;

struct MapFactoryAdapter<'a, P> {
    platform: &'a mut P,
}

impl<P: FrontendFactoryHost> MapFactoryHost for MapFactoryAdapter<'_, P> {
    type Error = P::Error;
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<crate::frontend_bindings::FrontendDevice>, Self::Error> {
        self.platform.enumerate_calibrated_devices(input)
    }
}

impl<P> PortSession<P> {
    pub fn map_screen(&self) -> Option<&MapScreen> {
        self.map.as_ref()
    }
}

impl<P: PortPlatform> PortSession<P> {
    /// Factory `00450FC0`. The centre starts at `0046B030`'s vehicle: the
    /// first player's `+D4`, its X and Z WORDs.
    pub(super) fn construct_map(
        &mut self,
        state: &SessionState,
    ) -> Result<(), PortSessionErrorOf<P>> {
        if self.map.is_some() {
            return Err(PortSessionError::ScreenAlreadyPresent(PortScreen::Map));
        }
        let vehicle_words = match self.world.as_mut() {
            Some(world) => match world
                .arena
                .first_player_primary_actor()
                .map_err(|error| PortSessionError::Map(format!("0046B030: {error}")))?
            {
                Some(vehicle) => {
                    let words = world
                        .arena
                        .object(vehicle)
                        .map_err(|error| PortSessionError::Map(error.to_string()))?
                        .base
                        .position_words
                        .ok_or_else(|| {
                            PortSessionError::Map(
                                "00451156 reads the vehicle's unwritten +32/+36".into(),
                            )
                        })?;
                    Some([words[0], words[2]])
                }
                None => None,
            },
            None => None,
        };
        let assets = self.catalog.assets();
        let mut screen = MapScreen::new(
            MapScreenConfig {
                attachment: self.attachment,
                callback_context: MAP_CALLBACK_CONTEXT,
            },
            assets,
        );
        let result = screen.initialize(
            &mut MapFactoryServices {
                registry: &mut self.registry,
                input: &mut self.input,
                assets,
            },
            &mut MapFactoryAdapter {
                platform: &mut self.platform,
            },
            state,
            vehicle_words,
        );
        // A failed factory keeps its partial acquisitions for explicit
        // teardown, as the construction contract requires.
        self.map = Some(screen);
        result.map_err(|error| PortSessionError::Map(error.to_string()))
    }
}
