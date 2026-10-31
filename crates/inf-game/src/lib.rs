//! Infestation runtime foundations, independent of the diagnostic viewer.
//!
//! Isolated scene preparation and authored Group/Select residency share one
//! physical loading boundary. The native resource owner preserves activation,
//! unloading and selector reload order; resource loading does not execute
//! screen/actor/backend callbacks. See 004EE1E0, 004EE680, 004EE6D0 and 004EFC00.
//! `session` and `scheduler` implement native dispatch/timing against explicit
//! host callbacks. `screen_registry` and `input` own native registration and
//! digital/untimed analog dispatch. `screen_input` owns held-mask repeat, and
//! frontend/menu owners execute authored rows against required render effects.
//! `frontend_screen` composes the frontend factory and live input/controller;
//! frame orchestration, native label layout, model geometry and CD music have
//! concrete policies with required backend operations. `frontend_app` joins
//! the actual screen, authored menu pixels, keyboard bindings and audio to SDL.
//! `player_identity` owns the native identity caches, registry fallback chains
//! and the `%dup` initials default behind the phase-9..11 constructor reads.
//! Specialized menus, session/title wiring, platform game input and world
//! backends remain open.

pub mod ambience;
pub mod attract_cycle;
pub mod audio_out;
pub mod briefing_camera;
pub mod briefing_controls;
pub mod briefing_draw;
pub mod briefing_map;
pub mod briefing_map_annotations;
pub mod briefing_markers;
pub mod briefing_renderer;
pub mod briefing_runtime;
pub mod briefing_scene;
pub mod briefing_screen;
pub mod briefing_sprites;
pub mod briefing_terrain;
pub mod camera_clearance;
pub mod cloud_wake;
pub mod cpu_profile;
pub mod craft_spray;
pub mod crash_log;
pub mod crystal_flares;
pub mod d3d_d2;
pub mod d3d_d6;
pub mod d3d_da;
pub mod d3d_effect;
pub mod d3d_flat;
pub mod d3d_shared_shade_uv;
pub mod d3d_solid_quad;
pub mod d3d_terrain;
pub mod d3d_texture;
pub mod d3d_textured;
pub mod d3d_textured_shade_uv;
pub mod d3d_textured_triangle;
pub mod d3d_textured_uv;
pub mod d3d_viewport;
pub mod disc_image;
pub mod display_selection;
pub mod draw_queue;
mod error;
pub mod event_log;
pub mod factory_shop;
pub mod factory_shop_draw;
pub mod fast_hash;
pub mod frame_device;
pub mod free_look_view;
pub mod fresh_heap;
pub mod frontend_app;
pub mod frontend_assets;
pub mod frontend_audio;
pub mod frontend_bindings;
pub mod frontend_camera;
pub mod frontend_control;
pub mod frontend_dispatch;
pub mod frontend_display;
pub mod frontend_frame;
pub mod frontend_renderer;
pub mod frontend_runtime;
pub mod frontend_scene;
pub mod frontend_screen;
pub mod frontend_settings;
pub mod frontend_sound;
pub mod frontend_video;
pub mod gameplay_menu;
pub mod gameplay_overlay;
pub mod gameplay_screen;
mod gl;
pub mod gl_device;
pub mod grant_text;
pub mod harness_start;
pub mod input;
mod installation;
pub mod intro_results_screen;
pub mod intro_sequence;
pub mod key_bindings;
pub mod key_config_draw;
pub mod key_config_screen;
pub mod key_text;
pub mod launcher_setup;
pub mod loading_draw;
pub mod loading_integration;
pub mod loading_model;
pub mod loading_read;
pub mod loading_runtime;
pub mod map_screen;
pub mod menu;
pub mod menu_basis;
pub mod menu_text;
pub mod missile_view;
mod model_auxiliary;
pub mod model_commands;
pub mod model_compute_2d;
pub mod model_context;
pub mod model_execution;
pub mod model_lighting;
pub mod model_primitives;
pub mod model_process;
mod model_process_publication;
pub mod model_projection;
pub mod model_scratch;
pub mod model_values;
pub mod model_vertices;
pub mod modern_settings;
pub mod modern_tuning;
pub mod name_entry;
pub mod name_entry_draw;
pub mod name_entry_frame;
pub mod name_entry_presentation;
pub mod name_entry_renderer;
pub mod name_entry_scene;
pub mod native_pools;
pub mod network;
pub mod network_hud;
pub mod network_menu;
pub mod object_runtime;
pub mod offline_prepare;
pub mod panel_animation;
pub mod panel_frame;
pub mod pelagia;
pub mod pixel_backend;
pub mod platform_audio;
pub mod platform_input;
pub mod platform_pad;
pub mod player_hud;
pub mod player_identity;
pub mod player_input_source;
pub mod player_message_draw;
pub mod player_prepare;
pub mod port_text;
pub mod presentation;
pub mod process_model_graphics;
pub mod process_particles;
pub mod process_sound;
pub mod process_tasks;
pub mod profile_draw;
pub mod profile_menu;
mod profile_name_format;
pub mod profile_store;
pub mod radar_blips;
pub mod raster_metal;
pub mod raster_motion;
pub mod render_resources;
pub mod replay_controls;
mod residency;
mod residency_tree;
mod resources;
pub mod results_credits;
pub mod results_page;
pub mod results_tick;
mod round_parts;
mod scene;
pub mod scene_capture;
pub mod scene_feed;
pub mod scene_field;
pub mod scene_gate;
pub mod scene_ground_detail;
pub mod scene_quad;
pub mod scene_resources;
pub mod scene_spray;
pub mod scene_sprite;
pub mod scene_terrain;
pub mod scene_trails;
pub mod scene_water;
pub mod scheduler;
pub mod screen_input;
pub mod screen_registry;
pub mod screen_renderer_preparation;
pub mod session;
pub(crate) mod session_capture;
pub mod session_effects;
pub mod session_log;
pub mod session_platform;
pub mod settings_row;
pub mod shown_menu_draw;
pub mod software_frame;
pub mod software_menu;
pub mod software_scene;
pub mod startup_draw;
pub mod startup_screen;
pub mod terrain_draw;
pub mod terrain_furniture;
pub mod terrain_map;
pub mod terrain_radar;
pub mod terrain_runtime;
pub mod text_entry_screen;
pub mod title_playback;
pub mod title_screen;
mod ultra_footings;
pub mod vehicle_hud;
pub mod vk_device;
pub mod word_resources;
pub mod world_actor_collision;
pub mod world_actor_contact;
pub mod world_actor_radius_effect;
pub mod world_actor_terrain;
pub mod world_backend;
pub mod world_captions;
pub mod world_completion;
pub mod world_fade;
pub mod world_flash;
pub mod world_furniture_effect;
pub mod world_inspection;
pub mod world_lighting;
pub mod world_loader;
pub mod world_map;
pub mod world_mission;
pub mod world_placement;
pub mod world_projectile_collision;
pub mod world_projectile_furniture;
pub mod world_radius_effect;
pub mod world_scene;
pub mod world_sight;
pub mod world_start;
pub mod world_statistics;
pub mod world_tint;
pub mod world_weather;

pub use error::{Error, Result};
pub use inf_formats::SceneContext;
pub use installation::{retail_path, Installation};
pub use residency::{
    ResidencyChange, ResidencyEvent, ResidencyRequest, ResidentRegistration, ResidentResource,
    ResidentRuntime, ResidentSummary, ResourceSelection,
};
pub use resources::HandlerResources;
pub use scene::{
    HandlerSummary, LoadedOverlay, LoadedScene, OverlaySummary, ResourceRegistration,
    ResourceScope, SceneSummary,
};

/// Owner of one independently prepared scene. Replacement is transactional.
///
/// This is a Rust ownership boundary, not a reconstruction of retail scene-tree
/// activation/unloading. In particular, no previous scene resources are carried
/// forward implicitly.
#[derive(Debug)]
pub struct Runtime {
    installation: Installation,
    scene: Option<LoadedScene>,
}

impl Runtime {
    pub fn new(installation: Installation) -> Self {
        Self {
            installation,
            scene: None,
        }
    }

    pub fn installation(&self) -> &Installation {
        &self.installation
    }

    pub fn scene(&self) -> Option<&LoadedScene> {
        self.scene.as_ref()
    }

    /// Commit only after every required phase has parsed and registered.
    /// Any failure retains the previous scene and its owned bytes untouched.
    pub fn load_scene(&mut self, context: SceneContext) -> Result<&LoadedScene> {
        let prepared = self.installation.prepare_scene(context)?;
        Ok(self.scene.insert(prepared))
    }
}

#[cfg(test)]
mod residency_tests;
#[cfg(test)]
mod test_worlds;
#[cfg(test)]
mod tests;
