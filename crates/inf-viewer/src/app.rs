//! SDL application shell and workbench interaction state.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::{Parser, ValueEnum};
use inf_formats::{SpriteRenderMode, SpriteShade};
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::mouse::{MouseButton, MouseWheelDirection};
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::camera::OrbitCamera;
pub use crate::capture_view::CaptureViewMode;
use crate::capture_view::LoadedModelObservation;
use crate::catalog::{default_data_root, step_index, AssetCatalog, ModelSelection};
use crate::input::{NavigationDirection, NavigationRepeat, PointerAction, PointerInput};
use crate::inspection_eye::EyeControls;
pub use crate::inspection_eye::InspectionEye;
use crate::model_presentation::ModelPresentation;
use crate::render::{QuadSplit, TerrainChannel};
use crate::ui::{Tab, UiAction, UiLayout};
use crate::view::{ViewContext, ViewState};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum StartupMode {
    Models,
    Sprites,
    Terrain,
}

impl From<StartupMode> for Tab {
    fn from(value: StartupMode) -> Self {
        match value {
            StartupMode::Models => Self::Models,
            StartupMode::Sprites => Self::Sprites,
            StartupMode::Terrain => Self::Terrain,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum StartupModelPresentation {
    LocalShell,
    NeutralComposite,
    ExplicitEye,
}

impl From<StartupModelPresentation> for ModelPresentation {
    fn from(value: StartupModelPresentation) -> Self {
        match value {
            StartupModelPresentation::LocalShell => Self::LocalShell,
            StartupModelPresentation::NeutralComposite => Self::NeutralComposite,
            StartupModelPresentation::ExplicitEye => Self::ExplicitEye,
        }
    }
}

#[derive(Clone, Debug, Parser)]
#[command(
    name = "inf-viewer",
    about = "Direct PreLoad/OVL diagnostic workbench (no extracted assets)"
)]
pub struct Args {
    /// Initial workbench tab.
    #[arg(long, value_enum)]
    pub mode: Option<StartupMode>,

    /// Initial model name, global ID (#123/0x7b), or source OVL search.
    #[arg(long)]
    pub model: Option<String>,

    /// Initial model evidence tier within the Models tab.
    #[arg(long, value_enum)]
    pub presentation: Option<StartupModelPresentation>,

    /// Explicit diagnostic eye X,Y,Z in root-model coordinates; selects the explicit-eye tier.
    #[arg(
        long,
        value_name = "X,Y,Z",
        allow_hyphen_values = true,
        conflicts_with = "capture"
    )]
    pub inspection_eye: Option<InspectionEye>,

    /// Native view-plane coordinate shift (independent of orbit/zoom).
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=31), conflicts_with = "capture")]
    pub eye_shift: u8,

    /// Restrict the catalog to one OVL name or path.
    #[arg(long)]
    pub ovl: Option<PathBuf>,

    /// Installation root containing PreLoad.dat and Overlay/.
    #[arg(long)]
    pub data_root: Option<PathBuf>,

    /// Verified HealthSmallDraw query bundle; repeat to compare observations.
    #[arg(long, conflicts_with_all = ["mode", "model", "presentation"])]
    pub capture: Vec<PathBuf>,

    /// One-based initial observation within the supplied capture bundles.
    #[arg(long, requires = "capture")]
    pub capture_index: Option<usize>,

    /// Initial captured geometry view.
    #[arg(long, value_enum, requires = "capture")]
    pub capture_view: Option<CaptureViewMode>,

    /// Save one workbench frame as PNG in a hidden window, then exit.
    #[arg(long)]
    pub snapshot: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<(), String> {
    let data_root = args.data_root.clone().unwrap_or_else(default_data_root);
    let catalog = AssetCatalog::load(&data_root, args.ovl.as_deref()).map_err(|error| {
        format!(
            "failed to build direct asset catalog from {}: {error}",
            data_root.display()
        )
    })?;
    eprintln!(
        "inf-viewer: loaded {} model slots ({} content variants, {} registrations), {} textures, {} materials, and {} terrain entries from {}",
        catalog.models.len(),
        catalog.model_variant_count(),
        catalog.model_occurrence_count(),
        catalog.sprites.len(),
        catalog.materials.len(),
        catalog.terrain.len(),
        data_root.display()
    );
    let mut state = AppState::new(&catalog, &args);
    state.load_captures(&catalog, &args)?;

    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let mut builder = video.window("Infestation raw asset workbench", 1_280, 800);
    builder.position_centered().resizable().allow_highdpi();
    if args.snapshot.is_some() {
        builder.hidden();
    }
    let mut window = builder.build().map_err(|error| error.to_string())?;
    window
        .set_minimum_size(760, 520)
        .map_err(|error| error.to_string())?;
    let mut canvas = window
        .into_canvas()
        .software()
        .build()
        .map_err(|error| error.to_string())?;
    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    if let Some(path) = &args.snapshot {
        state.draw(&mut canvas, &catalog, &data_root)?;
        return save_snapshot(&canvas, path);
    }
    video.text_input().start();
    let mut events = sdl.event_pump()?;

    'running: loop {
        for event in events.poll_iter() {
            if !state.handle_event(event, &canvas, &catalog) {
                break 'running;
            }
        }
        state.advance_held_navigation(Instant::now(), &catalog);
        state.draw(&mut canvas, &catalog, &data_root)?;
        canvas.present();
        std::thread::sleep(Duration::from_millis(16));
    }
    Ok(())
}

#[derive(Debug)]
struct AppState {
    tab: Tab,
    model_index: usize,
    model_variant_index: usize,
    model_occurrence_index: Option<usize>,
    model_presentation: ModelPresentation,
    inspection_eye: Option<InspectionEye>,
    eye_shift: u8,
    inspection_eye_text: String,
    captures: Vec<LoadedModelObservation>,
    capture_index: usize,
    capture_view: CaptureViewMode,
    sprite_index: usize,
    terrain_index: usize,
    camera: OrbitCamera,
    pointer: PointerInput,
    navigation_repeat: NavigationRepeat,
    search_text: String,
    model_position_text: String,
    text_input_focus: TextInputFocus,
    model_textures: bool,
    model_texture_difficulty: u8,
    model_texture_mode: SpriteRenderMode,
    model_quad_split: QuadSplit,
    sprite_mode: SpriteRenderMode,
    sprite_variant_override: Option<usize>,
    sprite_shade: SpriteShade,
    terrain_channel: TerrainChannel,
    status_message: String,
    view: ViewState,
}

impl AppState {
    fn new(catalog: &AssetCatalog, args: &Args) -> Self {
        let mut state = Self {
            tab: args.mode.map(Into::into).unwrap_or_else(|| {
                if args.model.is_some() {
                    Tab::Models
                } else {
                    Tab::default()
                }
            }),
            model_index: 0,
            model_variant_index: catalog
                .models
                .first()
                .map_or(0, |asset| asset.default_variant),
            model_occurrence_index: None,
            model_presentation: if args.inspection_eye.is_some() {
                ModelPresentation::ExplicitEye
            } else {
                args.presentation.map(Into::into).unwrap_or_default()
            },
            inspection_eye: args.inspection_eye,
            eye_shift: args.eye_shift,
            inspection_eye_text: String::new(),
            captures: Vec::new(),
            capture_index: 0,
            capture_view: args.capture_view.unwrap_or_default(),
            sprite_index: 0,
            terrain_index: 0,
            camera: OrbitCamera::default(),
            pointer: PointerInput::default(),
            navigation_repeat: NavigationRepeat::default(),
            search_text: String::new(),
            model_position_text: String::new(),
            text_input_focus: TextInputFocus::None,
            model_textures: true,
            model_texture_difficulty: 0,
            model_texture_mode: SpriteRenderMode::Mode0,
            model_quad_split: QuadSplit::default(),
            sprite_mode: SpriteRenderMode::Mode0,
            sprite_variant_override: None,
            sprite_shade: SpriteShade::Base,
            terrain_channel: TerrainChannel::Height,
            status_message: String::new(),
            view: ViewState::default(),
        };
        if let Some(query) = args.model.as_deref() {
            if let Some(selection) = catalog.find_model(query, None) {
                state.model_index = selection.model_index;
                state.model_variant_index = selection.variant_index;
                state.model_occurrence_index = selection.occurrence_index;
                state.search_text = query.to_owned();
            } else {
                state.status_message = format!("MODEL SEARCH DID NOT FIND: {query}");
            }
        }
        state
    }

    fn load_captures(&mut self, catalog: &AssetCatalog, args: &Args) -> Result<(), String> {
        let captures = args
            .capture
            .iter()
            .map(|path| LoadedModelObservation::load(path, catalog))
            .collect::<Result<Vec<_>, _>>()?;
        if captures.is_empty() {
            return Ok(());
        }
        let index = args
            .capture_index
            .unwrap_or(1)
            .checked_sub(1)
            .filter(|index| *index < captures.len())
            .ok_or_else(|| format!("capture index must be in 1..={}", captures.len()))?;
        self.select_model_variant(captures[index].selection);
        self.captures = captures;
        self.capture_index = index;
        self.model_presentation = ModelPresentation::CapturedDraw;
        self.tab = Tab::Models;
        Ok(())
    }

    fn is_captured_view(&self) -> bool {
        self.tab == Tab::Models && self.model_presentation == ModelPresentation::CapturedDraw
    }

    fn handle_event(
        &mut self,
        event: Event,
        canvas: &Canvas<Window>,
        catalog: &AssetCatalog,
    ) -> bool {
        match event {
            Event::Quit { .. } => return false,
            Event::KeyDown {
                keycode: Some(key),
                repeat: false,
                ..
            } => {
                if !self.handle_key(key, Instant::now(), catalog) {
                    return false;
                }
            }
            Event::KeyUp {
                keycode: Some(key), ..
            } => match key {
                Keycode::Left => self
                    .navigation_repeat
                    .release(NavigationDirection::Previous, Instant::now()),
                Keycode::Right => self
                    .navigation_repeat
                    .release(NavigationDirection::Next, Instant::now()),
                _ => {}
            },
            Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => {
                self.navigation_repeat.clear();
                self.cancel_text_input();
            }
            Event::TextInput { text, .. } => match self.text_input_focus {
                TextInputFocus::Search => {
                    if self.search_text.chars().count() < 64 {
                        self.search_text.push_str(&text);
                    }
                }
                TextInputFocus::ModelPosition => {
                    append_model_position_digits(&mut self.model_position_text, &text)
                }
                TextInputFocus::InspectionEye => {
                    for character in text
                        .chars()
                        .filter(|c| c.is_ascii_digit() || matches!(c, ',' | '-' | '+' | ' '))
                    {
                        if self.inspection_eye_text.len() < 48 {
                            self.inspection_eye_text.push(character);
                        }
                    }
                }
                TextInputFocus::None => {}
            },
            Event::MouseButtonDown {
                mouse_btn, x, y, ..
            } => self.mouse_button_down(mouse_btn, x, y, canvas, catalog),
            Event::MouseButtonUp { mouse_btn, .. } => match mouse_btn {
                MouseButton::Right => self.pointer.set_right(false),
                MouseButton::Middle => self.pointer.set_middle(false),
                _ => {}
            },
            Event::MouseMotion { xrel, yrel, .. } if self.tab == Tab::Models => {
                match self.pointer.motion(xrel, yrel) {
                    PointerAction::Orbit { dx, dy } => {
                        if !self.is_captured_view()
                            || self.capture_view == CaptureViewMode::SourceGeometry
                        {
                            self.camera.orbit(dx as f32, dy as f32);
                        }
                    }
                    PointerAction::Pan { dx, dy } => self.camera.pan(dx as f32, dy as f32),
                    PointerAction::None => {}
                }
            }
            Event::MouseWheel { y, direction, .. } if self.tab == Tab::Models => {
                let y = if direction == MouseWheelDirection::Flipped {
                    -y
                } else {
                    y
                };
                self.camera.zoom_by_wheel(y);
            }
            _ => {}
        }
        true
    }

    fn mouse_button_down(
        &mut self,
        button: MouseButton,
        x: i32,
        y: i32,
        canvas: &Canvas<Window>,
        catalog: &AssetCatalog,
    ) {
        match button {
            MouseButton::Left => {
                let (width, height) = canvas.output_size().unwrap_or((1_280, 800));
                let layout = UiLayout::new(width, height);
                if self.tab == Tab::Models
                    && self.model_presentation == ModelPresentation::ExplicitEye
                {
                    if let Some(action) = EyeControls::new(&layout).action_at(x, y) {
                        self.apply_action(action, catalog);
                        return;
                    }
                }
                if self.is_captured_view() {
                    if layout.model_variant.contains(x, y) {
                        self.capture_view = self.capture_view.next();
                        self.camera = OrbitCamera::default();
                        return;
                    }
                    if layout.reset_position.contains(x, y) {
                        self.camera = OrbitCamera::default();
                        return;
                    }
                    if layout.sidebar.contains(x, y) {
                        return;
                    }
                }
                let model_variant_enabled = catalog
                    .models
                    .get(self.model_index)
                    .is_some_and(|asset| asset.variants.len() > 1);
                if let Some(action) =
                    layout.action_at(self.tab, self.camera.has_pan(), model_variant_enabled, x, y)
                {
                    if !matches!(action, UiAction::FocusSearch | UiAction::FocusModelPosition) {
                        self.cancel_text_input();
                    }
                    self.apply_action(action, catalog);
                } else {
                    self.cancel_text_input();
                }
            }
            MouseButton::Right => self.pointer.set_right(true),
            MouseButton::Middle => self.pointer.set_middle(true),
            _ => {}
        }
    }

    fn handle_key(&mut self, key: Keycode, now: Instant, catalog: &AssetCatalog) -> bool {
        if self.handle_text_input_key(key, catalog) {
            return true;
        }
        match key {
            Keycode::Escape => return false,
            Keycode::F1 => self.apply_action(UiAction::SetTab(Tab::Models), catalog),
            Keycode::F2 => self.apply_action(UiAction::SetTab(Tab::Sprites), catalog),
            Keycode::F3 => self.apply_action(UiAction::SetTab(Tab::Terrain), catalog),
            Keycode::Left => {
                if self
                    .navigation_repeat
                    .press(NavigationDirection::Previous, now)
                {
                    self.navigate(NavigationDirection::Previous.delta(), catalog);
                }
            }
            Keycode::Right => {
                if self.navigation_repeat.press(NavigationDirection::Next, now) {
                    self.navigate(NavigationDirection::Next.delta(), catalog);
                }
            }
            Keycode::PageUp => self.apply_action(UiAction::Previous, catalog),
            Keycode::PageDown => self.apply_action(UiAction::Next, catalog),
            Keycode::A if self.tab == Tab::Models => {
                self.apply_action(UiAction::CycleModelPresentation, catalog)
            }
            Keycode::E
                if self.tab == Tab::Models
                    && self.model_presentation == ModelPresentation::ExplicitEye =>
            {
                self.apply_action(UiAction::FocusInspectionEye, catalog)
            }
            Keycode::C if self.is_captured_view() => {
                self.capture_view = self.capture_view.next();
                self.camera = OrbitCamera::default();
            }
            Keycode::R if self.is_captured_view() => self.camera = OrbitCamera::default(),
            Keycode::R if self.tab == Tab::Models && self.camera.has_pan() => {
                self.camera.reset_position()
            }
            Keycode::V if self.tab == Tab::Models => {
                self.apply_action(UiAction::CycleModelVariant, catalog)
            }
            Keycode::T if self.tab == Tab::Models => {
                self.apply_action(UiAction::ToggleModelTextures, catalog)
            }
            Keycode::D if self.tab == Tab::Models => {
                self.apply_action(UiAction::CycleModelTextureDifficulty, catalog)
            }
            Keycode::M if self.tab == Tab::Models => {
                self.apply_action(UiAction::CycleModelTextureMode, catalog)
            }
            Keycode::Q if self.tab == Tab::Models => {
                self.apply_action(UiAction::CycleModelQuadSplit, catalog)
            }
            Keycode::M if self.tab == Tab::Sprites => {
                self.apply_action(UiAction::CycleSpriteMode, catalog)
            }
            Keycode::V if self.tab == Tab::Sprites => {
                self.apply_action(UiAction::CycleSpriteVariant, catalog)
            }
            Keycode::S if self.tab == Tab::Sprites => {
                self.apply_action(UiAction::CycleSpriteShade, catalog)
            }
            Keycode::C if self.tab == Tab::Terrain => {
                self.apply_action(UiAction::CycleTerrainChannel, catalog)
            }
            Keycode::Slash if self.tab == Tab::Models => {
                self.apply_action(UiAction::FocusSearch, catalog)
            }
            Keycode::G if self.tab == Tab::Models => {
                self.apply_action(UiAction::FocusModelPosition, catalog)
            }
            _ => {}
        }
        true
    }

    fn handle_text_input_key(&mut self, key: Keycode, catalog: &AssetCatalog) -> bool {
        let focus = self.text_input_focus;
        if focus == TextInputFocus::None {
            return false;
        }

        match key {
            Keycode::Escape => self.cancel_text_input(),
            Keycode::Delete if focus == TextInputFocus::InspectionEye => {
                self.inspection_eye_text.clear()
            }
            Keycode::F1 => self.apply_action(UiAction::SetTab(Tab::Models), catalog),
            Keycode::F2 => self.apply_action(UiAction::SetTab(Tab::Sprites), catalog),
            Keycode::F3 => self.apply_action(UiAction::SetTab(Tab::Terrain), catalog),
            Keycode::Backspace => match focus {
                TextInputFocus::Search => {
                    self.search_text.pop();
                }
                TextInputFocus::ModelPosition => {
                    self.model_position_text.pop();
                }
                TextInputFocus::InspectionEye => {
                    self.inspection_eye_text.pop();
                }
                TextInputFocus::None => unreachable!(),
            },
            Keycode::Return | Keycode::KpEnter => match focus {
                TextInputFocus::Search => {
                    self.submit_model_search(catalog);
                    self.cancel_text_input();
                }
                TextInputFocus::ModelPosition => {
                    self.accept_model_position(catalog);
                }
                TextInputFocus::InspectionEye => {
                    match self.inspection_eye_text.parse::<InspectionEye>() {
                        Ok(eye) => {
                            self.inspection_eye = Some(eye);
                            self.status_message.clear();
                            self.cancel_text_input();
                        }
                        Err(error) => self.status_message = error,
                    }
                }
                TextInputFocus::None => unreachable!(),
            },
            _ => {}
        }
        true
    }

    fn focus_text_input(&mut self, focus: TextInputFocus) {
        debug_assert_ne!(focus, TextInputFocus::None);
        if focus == TextInputFocus::ModelPosition {
            self.model_position_text.clear();
        }
        if focus == TextInputFocus::InspectionEye {
            self.inspection_eye_text = self
                .inspection_eye
                .map(|eye| eye.to_string())
                .unwrap_or_default();
        }
        self.text_input_focus = focus;
        self.navigation_repeat.clear();
    }

    fn cancel_text_input(&mut self) {
        self.text_input_focus = TextInputFocus::None;
    }

    fn apply_action(&mut self, action: UiAction, catalog: &AssetCatalog) {
        if self.is_captured_view()
            && matches!(
                action,
                UiAction::FocusSearch
                    | UiAction::FocusModelPosition
                    | UiAction::CycleModelVariant
                    | UiAction::ToggleModelTextures
                    | UiAction::CycleModelTextureDifficulty
                    | UiAction::CycleModelTextureMode
                    | UiAction::CycleModelQuadSplit
            )
        {
            return;
        }
        match action {
            UiAction::SetTab(tab) => {
                self.tab = tab;
                self.cancel_text_input();
                self.navigation_repeat.clear();
            }
            UiAction::Previous => self.navigate(-1, catalog),
            UiAction::Next => self.navigate(1, catalog),
            UiAction::FocusSearch if self.tab == Tab::Models => {
                self.focus_text_input(TextInputFocus::Search);
            }
            UiAction::FocusSearch => {}
            UiAction::FocusModelPosition if self.tab == Tab::Models => {
                self.focus_text_input(TextInputFocus::ModelPosition);
            }
            UiAction::FocusModelPosition => {}
            UiAction::CycleModelPresentation => {
                self.model_presentation = self.model_presentation.next();
                self.cancel_text_input();
                if self.is_captured_view() {
                    if let Some(capture) = self.captures.get(self.capture_index) {
                        self.select_model_variant(capture.selection);
                    }
                }
            }
            UiAction::FocusInspectionEye => {
                if self.tab == Tab::Models
                    && self.model_presentation == ModelPresentation::ExplicitEye
                {
                    self.focus_text_input(TextInputFocus::InspectionEye);
                }
            }
            UiAction::AdjustInspectionEye { axis, delta } => {
                if self.tab == Tab::Models
                    && self.model_presentation == ModelPresentation::ExplicitEye
                {
                    if let Some(eye) = &mut self.inspection_eye {
                        eye.adjust(axis, delta);
                    }
                    self.cancel_text_input();
                }
            }
            UiAction::CycleInspectionShift => {
                if self.tab == Tab::Models
                    && self.model_presentation == ModelPresentation::ExplicitEye
                {
                    self.eye_shift = (self.eye_shift + 1) & 31;
                    self.cancel_text_input();
                }
            }
            UiAction::ResetPosition => self.camera.reset_position(),
            UiAction::CycleModelVariant => {
                if let Some(asset) = catalog.models.get(self.model_index) {
                    let count = asset.variants.len();
                    if count > 1 {
                        self.model_variant_index = (self.model_variant_index + 1) % count;
                        self.model_occurrence_index = None;
                        self.camera.reset_position();
                        self.status_message.clear();
                    }
                }
            }
            UiAction::ToggleModelTextures => self.model_textures = !self.model_textures,
            UiAction::CycleModelTextureDifficulty => {
                // Retail ships difficulty overlays d0..d4; the scene context
                // carries the selected value without enforcing the range.
                self.model_texture_difficulty = (self.model_texture_difficulty + 1) % 5;
            }
            UiAction::CycleModelTextureMode => {
                self.model_texture_mode = match self.model_texture_mode {
                    SpriteRenderMode::Mode0 => SpriteRenderMode::Mode1,
                    SpriteRenderMode::Mode1 => SpriteRenderMode::Mode0,
                };
            }
            UiAction::CycleModelQuadSplit => {
                self.model_quad_split = self.model_quad_split.next();
            }
            UiAction::CycleSpriteMode => {
                self.sprite_mode = match self.sprite_mode {
                    SpriteRenderMode::Mode0 => SpriteRenderMode::Mode1,
                    SpriteRenderMode::Mode1 => SpriteRenderMode::Mode0,
                };
                self.sprite_variant_override = None;
                self.view.invalidate_sprite();
            }
            UiAction::CycleSpriteVariant => {
                let count = catalog
                    .sprites
                    .get(self.sprite_index)
                    .map_or(0, |asset| asset.entry.variants.len());
                self.sprite_variant_override =
                    next_physical_variant(self.sprite_variant_override, count);
                self.view.invalidate_sprite();
            }
            UiAction::CycleSpriteShade => {
                self.sprite_shade = next_sprite_shade(self.sprite_shade);
                self.view.invalidate_sprite();
            }
            UiAction::CycleTerrainChannel => {
                self.terrain_channel = self.terrain_channel.next();
                self.view.invalidate_terrain();
            }
        }
    }

    fn navigate(&mut self, delta: isize, catalog: &AssetCatalog) {
        match self.tab {
            Tab::Models if self.is_captured_view() => {
                self.capture_index = step_index(self.capture_index, self.captures.len(), delta);
                if let Some(capture) = self.captures.get(self.capture_index) {
                    self.select_model_variant(capture.selection);
                }
            }
            Tab::Models => {
                let index = step_index(self.model_index, catalog.models.len(), delta);
                self.select_model(index, catalog);
            }
            Tab::Sprites => {
                self.sprite_index = step_index(self.sprite_index, catalog.sprites.len(), delta);
                self.sprite_variant_override = None;
                self.view.invalidate_sprite();
            }
            Tab::Terrain => {
                self.terrain_index = step_index(self.terrain_index, catalog.terrain.len(), delta);
                self.view.invalidate_terrain();
            }
        }
        self.status_message.clear();
    }

    fn advance_held_navigation(&mut self, now: Instant, catalog: &AssetCatalog) {
        if let Some(steps) = self.navigation_repeat.take_due(now) {
            self.navigate(steps.direction.delta() * steps.count as isize, catalog);
        }
    }

    fn submit_model_search(&mut self, catalog: &AssetCatalog) {
        if let Some(selection) = catalog.find_model(&self.search_text, Some(self.model_index)) {
            self.select_model_variant(selection);
        } else {
            self.status_message = format!("NO MODEL MATCH FOR: {}", self.search_text);
        }
    }

    fn submit_model_position(&mut self, catalog: &AssetCatalog) -> bool {
        let total = catalog.models.len();
        match parse_model_position(&self.model_position_text, total) {
            Ok(index) => {
                self.select_model(index, catalog);
                true
            }
            Err(ModelPositionError::Empty) => {
                self.status_message = format!("MODEL POSITION IS EMPTY (VALID RANGE: 1..={total})");
                false
            }
            Err(ModelPositionError::Invalid) => {
                self.status_message = format!(
                    "INVALID MODEL POSITION: {} (VALID RANGE: 1..={total})",
                    self.model_position_text
                );
                false
            }
            Err(ModelPositionError::OutOfRange { requested, total }) => {
                self.status_message = format!(
                    "MODEL POSITION {requested} IS OUT OF RANGE (VALID RANGE: 1..={total})"
                );
                false
            }
        }
    }

    fn accept_model_position(&mut self, catalog: &AssetCatalog) {
        if self.submit_model_position(catalog) {
            self.cancel_text_input();
        }
    }

    fn select_model(&mut self, index: usize, catalog: &AssetCatalog) {
        let variant_index = catalog
            .models
            .get(index)
            .map_or(0, |asset| asset.default_variant);
        self.select_model_variant(ModelSelection {
            model_index: index,
            variant_index,
            occurrence_index: None,
        });
    }

    fn select_model_variant(&mut self, selection: ModelSelection) {
        let ModelSelection {
            model_index,
            variant_index,
            occurrence_index,
        } = selection;
        self.model_index = model_index;
        self.model_variant_index = variant_index;
        self.model_occurrence_index = occurrence_index;
        self.camera.reset_position();
        self.status_message.clear();
    }

    fn draw(
        &mut self,
        canvas: &mut Canvas<Window>,
        catalog: &AssetCatalog,
        data_root: &Path,
    ) -> Result<(), String> {
        let context = ViewContext {
            tab: self.tab,
            model_index: self.model_index,
            model_variant_index: self.model_variant_index,
            model_occurrence_index: self.model_occurrence_index,
            model_presentation: self.model_presentation,
            inspection_eye: self.inspection_eye,
            eye_shift: self.eye_shift,
            inspection_eye_text: &self.inspection_eye_text,
            inspection_eye_active: self.text_input_focus == TextInputFocus::InspectionEye,
            capture: self.captures.get(self.capture_index),
            capture_index: self.capture_index,
            capture_count: self.captures.len(),
            capture_view: self.capture_view,
            sprite_index: self.sprite_index,
            terrain_index: self.terrain_index,
            camera: &self.camera,
            search_text: &self.search_text,
            search_active: self.text_input_focus == TextInputFocus::Search,
            model_position_text: &self.model_position_text,
            model_position_active: self.text_input_focus == TextInputFocus::ModelPosition,
            model_textures: self.model_textures,
            model_texture_difficulty: self.model_texture_difficulty,
            model_texture_mode: self.model_texture_mode,
            model_quad_split: self.model_quad_split,
            sprite_mode: self.sprite_mode,
            sprite_variant_override: self.sprite_variant_override,
            sprite_shade: self.sprite_shade,
            terrain_channel: self.terrain_channel,
            status_message: &self.status_message,
        };
        self.view.draw(canvas, catalog, data_root, &context)
    }
}

fn save_snapshot(canvas: &Canvas<Window>, path: &Path) -> Result<(), String> {
    let (width, height) = canvas.output_size()?;
    let pixels = canvas.read_pixels(None, sdl2::pixels::PixelFormatEnum::RGBA32)?;
    let file =
        std::fs::File::create(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer
        .write_image_data(&pixels)
        .map_err(|error| error.to_string())?;
    writer.finish().map_err(|error| error.to_string())?;
    eprintln!("inf-viewer: saved {}", path.display());
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TextInputFocus {
    #[default]
    None,
    Search,
    ModelPosition,
    InspectionEye,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelPositionError {
    Empty,
    Invalid,
    OutOfRange { requested: usize, total: usize },
}

const MAX_MODEL_POSITION_DIGITS: usize = 9;

fn append_model_position_digits(buffer: &mut String, text: &str) {
    for digit in text.chars().filter(char::is_ascii_digit) {
        if buffer.len() >= MAX_MODEL_POSITION_DIGITS {
            break;
        }
        buffer.push(digit);
    }
}

fn parse_model_position(input: &str, total: usize) -> Result<usize, ModelPositionError> {
    if input.is_empty() {
        return Err(ModelPositionError::Empty);
    }
    if !input.chars().all(|character| character.is_ascii_digit()) {
        return Err(ModelPositionError::Invalid);
    }
    let requested = input
        .parse::<usize>()
        .map_err(|_| ModelPositionError::Invalid)?;
    if requested == 0 || requested > total {
        return Err(ModelPositionError::OutOfRange { requested, total });
    }
    Ok(requested - 1)
}

fn next_physical_variant(current: Option<usize>, count: usize) -> Option<usize> {
    if count == 0 {
        None
    } else {
        match current {
            None => Some(0),
            Some(index) if index + 1 < count => Some(index + 1),
            Some(_) => None,
        }
    }
}

fn next_sprite_shade(shade: SpriteShade) -> SpriteShade {
    match shade {
        SpriteShade::Base => SpriteShade::Level(0),
        SpriteShade::Level(31) => SpriteShade::Base,
        SpriteShade::Level(level) => SpriteShade::Level(level + 1),
    }
}

#[cfg(test)]
#[path = "app_eye_tests.rs"]
mod eye_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_state() -> AppState {
        AppState::new(
            &AssetCatalog::default(),
            &Args {
                mode: None,
                model: None,
                presentation: None,
                inspection_eye: None,
                eye_shift: 0,
                ovl: None,
                data_root: None,
                capture: Vec::new(),
                capture_index: None,
                capture_view: None,
                snapshot: None,
            },
        )
    }

    #[test]
    fn physical_variant_cycle_returns_to_logical_auto_selection() {
        assert_eq!(next_physical_variant(None, 2), Some(0));
        assert_eq!(next_physical_variant(Some(0), 2), Some(1));
        assert_eq!(next_physical_variant(Some(1), 2), None);
        assert_eq!(next_physical_variant(None, 0), None);
    }

    #[test]
    fn shade_cycle_includes_base_and_all_thirty_two_levels() {
        let mut shade = SpriteShade::Base;
        for level in 0..32 {
            shade = next_sprite_shade(shade);
            assert_eq!(shade, SpriteShade::Level(level));
        }
        assert_eq!(next_sprite_shade(shade), SpriteShade::Base);
    }

    #[test]
    fn model_surface_controls_cycle_without_touching_sprite_state() {
        let catalog = AssetCatalog::default();
        let mut state = empty_state();
        assert!(state.model_textures);
        assert_eq!(state.model_texture_difficulty, 0);
        assert_eq!(state.model_texture_mode, SpriteRenderMode::Mode0);
        assert_eq!(state.model_quad_split, QuadSplit::Diagonal02);
        assert_eq!(state.sprite_mode, SpriteRenderMode::Mode0);

        state.apply_action(UiAction::ToggleModelTextures, &catalog);
        state.apply_action(UiAction::CycleModelTextureMode, &catalog);
        state.apply_action(UiAction::CycleModelQuadSplit, &catalog);
        for _ in 0..5 {
            state.apply_action(UiAction::CycleModelTextureDifficulty, &catalog);
        }

        assert!(!state.model_textures);
        assert_eq!(state.model_texture_difficulty, 0);
        assert_eq!(state.model_texture_mode, SpriteRenderMode::Mode1);
        assert_eq!(state.model_quad_split, QuadSplit::Diagonal13);
        assert_eq!(state.sprite_mode, SpriteRenderMode::Mode0);
    }

    #[test]
    fn model_position_parser_is_one_based_and_rejects_bad_ranges() {
        assert_eq!(parse_model_position("1", 5), Ok(0));
        assert_eq!(parse_model_position("5", 5), Ok(4));
        assert_eq!(parse_model_position("", 5), Err(ModelPositionError::Empty));
        assert_eq!(
            parse_model_position("2x", 5),
            Err(ModelPositionError::Invalid)
        );
        assert_eq!(
            parse_model_position("0", 5),
            Err(ModelPositionError::OutOfRange {
                requested: 0,
                total: 5,
            })
        );
        assert_eq!(
            parse_model_position("6", 5),
            Err(ModelPositionError::OutOfRange {
                requested: 6,
                total: 5,
            })
        );
    }

    #[test]
    fn model_position_editor_accepts_only_capped_ascii_digits() {
        let mut buffer = String::new();
        append_model_position_digits(&mut buffer, "12x٣34");
        append_model_position_digits(&mut buffer, "56789012345");
        assert_eq!(buffer, "123456789");
        assert_eq!(buffer.len(), MAX_MODEL_POSITION_DIGITS);
    }

    #[test]
    fn text_input_focus_is_exclusive_and_cancels_held_navigation() {
        let mut state = empty_state();
        let start = Instant::now();
        state
            .navigation_repeat
            .press(NavigationDirection::Next, start);

        state.focus_text_input(TextInputFocus::Search);
        assert_eq!(state.text_input_focus, TextInputFocus::Search);
        state.model_position_text = "old".to_owned();
        state.focus_text_input(TextInputFocus::ModelPosition);

        assert_eq!(state.text_input_focus, TextInputFocus::ModelPosition);
        assert!(state.model_position_text.is_empty());
        assert_eq!(
            state
                .navigation_repeat
                .take_due(start + Duration::from_secs(1)),
            None
        );
    }

    #[test]
    fn model_selection_resets_variant_and_camera_while_bad_position_stays_editable() {
        let mut state = empty_state();
        state.model_index = 1;
        state.model_variant_index = 7;
        state.camera.pan(12.0, -4.0);
        state.status_message = "OLD STATUS".to_owned();

        state.select_model_variant(ModelSelection {
            model_index: 4,
            variant_index: 2,
            occurrence_index: Some(6),
        });
        assert_eq!(state.model_index, 4);
        assert_eq!(state.model_variant_index, 2);
        assert_eq!(state.model_occurrence_index, Some(6));
        assert!(!state.camera.has_pan());
        assert!(state.status_message.is_empty());

        state.focus_text_input(TextInputFocus::ModelPosition);
        state.model_position_text = "9000".to_owned();
        state.accept_model_position(&AssetCatalog::default());
        assert_eq!(state.model_index, 4);
        assert_eq!(state.model_variant_index, 2);
        assert_eq!(state.model_occurrence_index, Some(6));
        assert_eq!(state.text_input_focus, TextInputFocus::ModelPosition);
        assert!(state.status_message.contains("OUT OF RANGE"));
    }

    #[test]
    fn ordinary_model_selection_clears_a_search_source_override() {
        let mut state = empty_state();
        state.select_model_variant(ModelSelection {
            model_index: 4,
            variant_index: 2,
            occurrence_index: Some(6),
        });

        state.select_model(0, &AssetCatalog::default());

        assert_eq!(state.model_occurrence_index, None);
    }

    #[test]
    fn model_variant_cycle_clears_a_search_source_override() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let catalog = AssetCatalog::load(&root, None).unwrap();
        let model_index = catalog
            .models
            .iter()
            .position(|asset| asset.variants.len() > 1)
            .expect("retail catalog has content variants");
        let mut state = AppState::new(
            &catalog,
            &Args {
                mode: None,
                model: None,
                presentation: None,
                inspection_eye: None,
                eye_shift: 0,
                ovl: None,
                data_root: None,
                capture: Vec::new(),
                capture_index: None,
                capture_view: None,
                snapshot: None,
            },
        );
        state.model_index = model_index;
        state.model_variant_index = 0;
        state.model_occurrence_index = Some(0);

        state.apply_action(UiAction::CycleModelVariant, &catalog);

        assert_eq!(state.model_variant_index, 1);
        assert_eq!(state.model_occurrence_index, None);
    }

    #[test]
    fn switching_model_presentation_preserves_model_selection() {
        let catalog = AssetCatalog::default();
        let mut state = empty_state();
        state.model_index = 42;
        state.model_variant_index = 3;
        state.model_occurrence_index = Some(7);

        state.apply_action(UiAction::CycleModelPresentation, &catalog);
        assert_eq!(
            state.model_presentation,
            ModelPresentation::NeutralComposite
        );
        assert_eq!(state.model_index, 42);
        assert_eq!(state.model_variant_index, 3);
        assert_eq!(state.model_occurrence_index, Some(7));

        state.apply_action(UiAction::CycleModelPresentation, &catalog);
        assert_eq!(state.model_presentation, ModelPresentation::ExplicitEye);
        state.apply_action(UiAction::CycleModelPresentation, &catalog);
        assert_eq!(state.model_presentation, ModelPresentation::CapturedDraw);
        assert_eq!(state.model_index, 42);
        assert_eq!(state.model_variant_index, 3);
        assert_eq!(state.model_occurrence_index, Some(7));

        state.apply_action(UiAction::CycleModelPresentation, &catalog);
        assert_eq!(state.model_presentation, ModelPresentation::LocalShell);
        assert_eq!(state.model_index, 42);
        assert_eq!(state.model_variant_index, 3);
        assert_eq!(state.model_occurrence_index, Some(7));
    }

    #[test]
    fn neutral_composite_is_an_explicit_startup_presentation() {
        assert_eq!(
            ModelPresentation::from(StartupModelPresentation::NeutralComposite),
            ModelPresentation::NeutralComposite
        );
    }

    #[test]
    fn captured_view_keeps_catalog_navigation_and_surface_controls_separate() {
        let catalog = AssetCatalog::default();
        let mut state = empty_state();
        state.model_index = 42;
        state.model_presentation = ModelPresentation::CapturedDraw;
        state.apply_action(UiAction::Next, &catalog);
        state.apply_action(UiAction::FocusSearch, &catalog);
        state.apply_action(UiAction::ToggleModelTextures, &catalog);
        assert_eq!(state.model_index, 42);
        assert_eq!(state.text_input_focus, TextInputFocus::None);
        assert!(state.model_textures);
        assert_eq!(state.capture_index, 0);
        state.handle_key(Keycode::C, Instant::now(), &catalog);
        assert_eq!(state.capture_view, CaptureViewMode::SourceGeometry);
        state.camera.zoom_by_wheel(3);
        state.camera.pan(14.0, 21.0);
        state.handle_key(Keycode::R, Instant::now(), &catalog);
        assert_eq!(state.camera, OrbitCamera::default());
    }

    #[test]
    fn capture_cli_is_explicit_and_accepts_multiple_bundles() {
        let args = Args::try_parse_from([
            "inf-viewer",
            "--capture",
            "first",
            "--capture",
            "second",
            "--capture-index",
            "2",
            "--capture-view",
            "source-geometry",
            "--snapshot",
            "view.png",
        ])
        .unwrap();
        assert_eq!(
            args.capture,
            [PathBuf::from("first"), PathBuf::from("second")]
        );
        assert_eq!(args.capture_index, Some(2));
        assert_eq!(args.capture_view, Some(CaptureViewMode::SourceGeometry));
        assert!(Args::try_parse_from(["inf-viewer", "--capture-index", "1"]).is_err());
        for (option, value) in [
            ("--model", "#168"),
            ("--mode", "models"),
            ("--presentation", "local-shell"),
        ] {
            assert!(
                Args::try_parse_from(["inf-viewer", "--capture", "first", option, value]).is_err()
            );
        }
    }

    #[test]
    fn function_keys_select_the_three_workbench_tabs() {
        let catalog = AssetCatalog::default();
        let mut state = empty_state();
        let now = Instant::now();

        for (key, expected) in [
            (Keycode::F1, Tab::Models),
            (Keycode::F2, Tab::Sprites),
            (Keycode::F3, Tab::Terrain),
        ] {
            assert!(state.handle_key(key, now, &catalog));
            assert_eq!(state.tab, expected);
        }
    }
}
