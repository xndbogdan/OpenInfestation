//! Scoped draw access across the session's screen and resident-resource owners.

use thiserror::Error;

use super::PortSession;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_renderer::FrontendRenderContext,
    frontend_screen::{FrontendProcessState, FrontendScreen},
    input::InputContextId,
    screen_registry::{
        RegistryCallbackError, RegistryError, ScreenAttachment, ScreenId, ScreenRegistry,
    },
    text_entry_screen::TextEntryScreen,
    title_screen::TitleScreen,
    ResidentRuntime,
};

/// Disjoint borrows for the frontend's +08 draw 00484940. `owner_byte` is
/// gameplay byte +5C2, which selects the menu visibility and profile rows.
/// The platform supplies the settings, video and profile owners the rows read.
pub struct PortFrontendDraw<'a, P> {
    pub screen: &'a mut FrontendScreen,
    pub registry: &'a mut ScreenRegistry<FrontendRenderContext, InputContextId>,
    pub attachment: ScreenAttachment,
    pub process: &'a FrontendProcessState,
    pub owner_byte: bool,
    pub platform: &'a mut P,
}

/// These disjoint borrows let the caller construct a resident renderer and
/// draw the actual screen without copying either residency or screen state.
/// The callback runs inside the registry's linked-context busy guard.
pub struct PortNameEntryDraw<'a> {
    pub screen: &'a mut TextEntryScreen,
    pub residency: &'a ResidentRuntime,
    pub assets: &'a FrontendAssets,
    pub registry: &'a mut ScreenRegistry<FrontendRenderContext, InputContextId>,
    pub attachment: ScreenAttachment,
    pub animation: AnimationClock,
}

/// Disjoint borrows for the title's +08 draw `004047F0`. The title owns no
/// menu, profile or residency inputs: the caller supplies the renderer the
/// arena preamble runs on. Presentation (+0C `00404840`) is empty and stays
/// outside this callback.
pub struct PortTitleDraw<'a, P> {
    pub screen: &'a mut TitleScreen,
    pub registry: &'a mut ScreenRegistry<FrontendRenderContext, InputContextId>,
    pub attachment: ScreenAttachment,
    pub platform: &'a mut P,
}

/// Phase-3 screen and the disjoint owners needed by its resident drawing.
pub struct PortBriefingDraw<'a, P> {
    pub screen: &'a mut crate::briefing_screen::BriefingScreen,
    pub process: &'a FrontendProcessState,
    pub process_rng: &'a mut u32,
    pub world: crate::briefing_renderer::BriefingWorld<'a>,
    pub effects: crate::briefing_renderer::BriefingProcessEffects<'a>,
    pub platform: &'a mut P,
}

#[cfg(test)]
#[path = "draw_tests.rs"]
mod tests;

#[derive(Debug, Error)]
pub enum PortLinkedDrawError<E> {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("drawing screen {expected:?} is not the registry's linked target {actual:?}")]
    LinkedScreen {
        expected: Option<ScreenId>,
        actual: Option<ScreenId>,
    },
    #[error("linked briefing draw requires the retained world's process RNG")]
    MissingWorld,
    #[error(transparent)]
    MapLookup(#[from] crate::world_map::WorldMapLookupError),
    #[error(transparent)]
    Executable(#[from] inf_formats::FrontendExecutableError),
    #[error("linked draw callback failed: {0}")]
    Callback(E),
}

impl<P> PortSession<P> {
    /// Phase-3 draw runs against its current linked screen under the same
    /// context guard as frontend/text drawing. The loaded world retains the
    /// shared process RNG until teardown; H8 jitter consumes that exact word,
    /// including reached random draws before a presentation failure.
    pub fn with_briefing_draw<T, E>(
        &mut self,
        phase: crate::session::SessionPhase,
        callback: impl FnOnce(PortBriefingDraw<'_, P>) -> Result<T, E>,
    ) -> Result<Option<T>, PortLinkedDrawError<E>> {
        let Some(screen) = self.briefing.as_mut() else {
            return Ok(None);
        };
        let process = &self.process;
        let world = &mut self.world;
        let maps = &self.maps;
        let network_active = self.factory_process.network_flag != 0;
        let factory_process = &self.factory_process;
        let owner = self.attachment.context_owner;
        let executable =
            inf_formats::FrontendExecutable::parse(self.catalog.assets().executable_bytes())?;
        let furniture_draw = &mut self.furniture_draw;
        let furniture_pending = &self.furniture;
        let light_grid = &mut self.light_grid;
        let particles = &mut self.particles;
        let object_draw_process = &mut self.object_draw_process;
        let weapon_tables = &mut self.actor_behavior_tables;
        let sounds = &mut self.sounds;
        let attract_demo = &self.attract;
        let residency = &self.residency;
        let platform = &mut self.platform;
        let pickup_text = crate::grant_text::PickupTextData {
            common: super::world_owners::current_grant_text(&self.profile_name_classification),
            hyphen_high: self
                .profile_name_classification
                .high_bytes
                .as_ref()
                .and_then(|bytes| bytes[b'-' as usize]),
        };
        let result = self.registry.with_context_dispatch(owner, |registry| {
            let actual = registry.linked_screen(owner)?;
            let expected = screen.id();
            if expected.is_none() || expected != actual {
                return Err(PortLinkedDrawError::LinkedScreen { expected, actual });
            }
            let world = world.as_mut().ok_or(PortLinkedDrawError::MissingWorld)?;
            callback(PortBriefingDraw {
                screen,
                process,
                process_rng: &mut world.rng,
                world: crate::briefing_renderer::BriefingWorld {
                    residency,
                    terrain_source: &world.terrain_source,
                    arena: &mut world.arena,
                    map_state: maps.jitter(world.map)?,
                    terrain: &mut world.terrain,
                    presentation: &mut world.presentation,
                    world_flags: world.world_flags,
                    network_active,
                },
                effects: crate::briefing_renderer::BriefingProcessEffects {
                    firing: Some(crate::briefing_renderer::BriefingWeaponFiringOwners {
                        tables: weapon_tables,
                        sounds,
                        statistics: world.mission.record_0c.as_deref_mut(),
                    }),
                    pickups: Some(crate::briefing_renderer::BriefingPickupOwners {
                        mission_save: &mut world.mission.save,
                        current_world: &world.mission.current_world_a0,
                        text: Some(pickup_text),
                    }),
                    object_draw_process,
                    attract_demo,
                    state: furniture_draw,
                    pending: furniture_pending,
                    light_grid,
                    particles,
                    executable,
                    birth_inputs: crate::process_particles::births::ParticleBirthInputs {
                        network_active,
                        phase: Some(u32::from(phase.id())),
                        process: factory_process,
                    },
                    load_factor: world.fade.performance.particle_load_factor as i32,
                },
                platform,
            })
            .map(Some)
            .map_err(PortLinkedDrawError::Callback)
        });
        flatten(result)
    }
    /// Dispatch the existing name-entry screen under its context guard.
    /// Returns `None` when the session owns another screen; in particular a
    /// 9->2 transition cannot draw the destroyed text-entry owner again.
    ///
    /// The caller owns presentation and renderer setup. Within this scoped
    /// callback call `screen.draw_frame` so failures stop the screen exactly
    /// as they do through its ordinary draw API.
    pub fn with_name_entry_draw<T, E>(
        &mut self,
        callback: impl FnOnce(PortNameEntryDraw<'_>) -> Result<T, E>,
    ) -> Result<Option<T>, PortLinkedDrawError<E>> {
        let Some(screen) = self.text.as_mut() else {
            return Ok(None);
        };
        let attachment = self.attachment;
        let residency = &self.residency;
        let assets = self.catalog.assets();
        let animation = self.process.animation();
        let result = self
            .registry
            .with_context_dispatch(attachment.context_owner, |registry| {
                let actual = registry.linked_screen(attachment.context_owner)?;
                let expected = screen.id();
                if expected.is_none() || expected != actual {
                    return Err(PortLinkedDrawError::LinkedScreen { expected, actual });
                }
                callback(PortNameEntryDraw {
                    screen,
                    residency,
                    assets,
                    registry,
                    attachment,
                    animation,
                })
                .map(Some)
                .map_err(PortLinkedDrawError::Callback)
            });
        flatten(result)
    }

    /// Dispatch the existing title screen under its context guard, as
    /// [`Self::with_name_entry_draw`] does for the text-entry screen. Returns
    /// `None` when the session owns another screen. Within the callback call
    /// `screen.draw` so failures stop the screen.
    pub fn with_title_draw<T, E>(
        &mut self,
        callback: impl FnOnce(PortTitleDraw<'_, P>) -> Result<T, E>,
    ) -> Result<Option<T>, PortLinkedDrawError<E>> {
        let Some(screen) = self.title.as_mut() else {
            return Ok(None);
        };
        let attachment = self.attachment;
        let platform = &mut self.platform;
        let result = self
            .registry
            .with_context_dispatch(attachment.context_owner, |registry| {
                let actual = registry.linked_screen(attachment.context_owner)?;
                let expected = screen.id();
                if expected.is_none() || expected != actual {
                    return Err(PortLinkedDrawError::LinkedScreen { expected, actual });
                }
                callback(PortTitleDraw {
                    screen,
                    registry,
                    attachment,
                    platform,
                })
                .map(Some)
                .map_err(PortLinkedDrawError::Callback)
            });
        flatten(result)
    }

    /// Dispatch the existing frontend screen under its context guard, as
    /// [`Self::with_name_entry_draw`] does for the text-entry screen. Returns
    /// `None` when the session owns no frontend screen. Within the callback
    /// call `screen.draw_frame` so failures stop the screen.
    pub fn with_frontend_draw<T, E>(
        &mut self,
        callback: impl FnOnce(PortFrontendDraw<'_, P>) -> Result<T, E>,
    ) -> Result<Option<T>, PortLinkedDrawError<E>> {
        let Some(screen) = self.frontend.as_mut() else {
            return Ok(None);
        };
        let attachment = self.attachment;
        let process = &self.process;
        let owner_byte = self.owner_byte;
        let platform = &mut self.platform;
        let result = self
            .registry
            .with_context_dispatch(attachment.context_owner, |registry| {
                let actual = registry.linked_screen(attachment.context_owner)?;
                let expected = screen.id();
                if expected.is_none() || expected != actual {
                    return Err(PortLinkedDrawError::LinkedScreen { expected, actual });
                }
                callback(PortFrontendDraw {
                    screen,
                    registry,
                    attachment,
                    process,
                    owner_byte,
                    platform,
                })
                .map(Some)
                .map_err(PortLinkedDrawError::Callback)
            });
        flatten(result)
    }
}

fn flatten<T, E>(
    result: Result<T, RegistryCallbackError<PortLinkedDrawError<E>>>,
) -> Result<T, PortLinkedDrawError<E>> {
    match result {
        Ok(value) => Ok(value),
        Err(RegistryCallbackError::Registry(error)) => Err(error.into()),
        Err(RegistryCallbackError::Callback(error)) => Err(error),
    }
}
