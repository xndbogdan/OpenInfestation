//! Concrete 0040B0F0 presentation phases over the shared resident renderer.
//! Ordinary markers and independent annotations use the retained world's
//! actor/map owners. Complete terrain/world composition has separate owners.

#[path = "briefing_renderer/feature_errors.rs"]
mod feature_errors;

use inf_formats::BriefingModel;
use thiserror::Error;

use crate::{
    briefing_draw::draw_briefing_menu,
    briefing_map::{briefing_map_backdrop, BriefingMapInputs},
    briefing_map_annotations::{
        briefing_map_annotation, BriefingAnnotationDraw, BriefingAnnotationError,
        BriefingAnnotationInputs, BriefingMapAnnotation,
    },
    briefing_markers::{
        BriefingMapActor, BriefingMarkerError, BriefingMarkerInputs, BriefingMarkerMaterialLookup,
        BriefingMarkerPass, BriefingMarkerTile,
    },
    briefing_runtime::{BriefingPanel, BriefingPanelAnimation},
    briefing_scene::{
        draw_briefing_decoration, draw_briefing_model_preview, draw_briefing_panel_frame,
        panel_visible, BriefingSceneFrame,
    },
    briefing_screen::{BriefingDrawError, BriefingScreen},
    briefing_sprites::{briefing_background_quads, briefing_progress_quads},
    briefing_terrain::{
        briefing_terrain_pane, BriefingNoiseError, BriefingNoisePass, BriefingNoiseRows,
        BriefingTerrainInputs, BriefingTerrainPane,
    },
    frontend_renderer::FrontendRendererState,
    frontend_scene::{FrontendSceneHost, SceneCommand, SceneFade, SceneProjection},
    frontend_screen::FrontendProcessState,
    menu::{MenuColor, MenuDrawInput, MenuError, MenuStack, MenuVisibility},
    menu_basis::MenuBasisMath,
    menu_text::{
        draw_line_text, measure_text_width, LineTextRequest, MenuTextEmission, MenuTextLayoutError,
        MenuTextResources, MenuTextReveal, RawMenuTextRequest, INITIAL_GLYPH_BASE_WORD,
    },
    render_resources::{
        RenderResourceData, RenderResourceError, RenderResourceKind, RenderResourceRequest,
        RenderResources,
    },
    scene_resources::{ResidentSceneHost, ResolvedSceneCommand, SceneResourceBackend},
    software_menu::{SoftwareMenu, SoftwareMenuError, SoftwareMenuRequest},
    software_scene::{SoftwareScene, SoftwareSceneError},
};

#[derive(Debug, Error)]
pub enum BriefingRendererError {
    #[error(transparent)]
    Screen(#[from] BriefingDrawError),
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error("briefing feature failed: {source}")]
    Feature {
        #[source]
        source: Box<dyn std::error::Error>,
    },
    #[error("the linked briefing has no bound H14 program")]
    MissingProgram,
    #[error("the linked briefing has no authored menu controllers")]
    MissingControls,
}

#[derive(Debug, Error)]
enum StartTextError {
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Layout(#[from] MenuTextLayoutError),
    #[error(transparent)]
    Menu(#[from] SoftwareMenuError),
}

#[derive(Debug, Error)]
enum MapMarkerDrawError {
    #[error(transparent)]
    Object(#[from] crate::object_runtime::ObjectError),
    #[error(transparent)]
    Marker(#[from] BriefingMarkerError<RenderResourceError>),
    #[error(transparent)]
    Annotation(#[from] BriefingAnnotationError<RenderResourceError>),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Menu(#[from] SoftwareMenuError),
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error("briefing marker drawing requires the retained process RNG")]
    MissingRng,
}

#[derive(Clone, Copy)]
struct BriefingTerrainEffects {
    camera_disabled: bool,
    pulse_remaining_micros: u32,
    report_omissions: bool,
}

#[derive(Debug, Error)]
enum TerrainStaticDrawError {
    #[error(transparent)]
    Noise(#[from] BriefingNoiseError<RenderResourceError>),
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error("briefing camera static requires the retained process RNG")]
    MissingRng,
}

/// Original fixed executable text at0052F1D4, not an H0 localization slot.
const CAMERA_DISABLED_TEXT: &[u8] = b"Camera disabled";

/// World owners read by 00472D30. These borrows belong to the retained world,
/// independently of screen lifetime and the selected camera actor.
pub struct BriefingWorld<'a> {
    pub(crate) residency: &'a crate::ResidentRuntime,
    pub(crate) terrain_source: &'a crate::world_actor_terrain::WorldTerrainSource,
    pub arena: &'a mut crate::object_runtime::ObjectArena,
    pub map_state: &'a crate::world_map::WorldMapState,
    pub terrain: &'a mut crate::terrain_runtime::TerrainRuntime,
    pub presentation: &'a mut crate::world_backend::WorldPresentationState,
    /// Retained H14 entry +4 flags, independently of terrain sampling policy.
    pub world_flags: u32,
    pub network_active: bool,
}

/// Process allocations outlive the briefing screen and world replacement.
/// 0040BE6F clears cells before the terrain prefix, while inherited anchors
/// remain intact. Furniture reads that grid during ground; 00479500's later
/// object callbacks accumulate into it before the shared flush and static.
pub struct BriefingProcessEffects<'a> {
    pub firing: Option<BriefingWeaponFiringOwners<'a>>,
    /// The mission owners a pickup's draw `0046DAF0` marks when collected.
    pub pickups: Option<BriefingPickupOwners<'a>>,
    pub object_draw_process: &'a mut crate::object_runtime::ObjectDrawProcessState,
    pub attract_demo: &'a bool,
    pub state: &'a mut crate::terrain_furniture::draw::FurnitureDrawState,
    pub pending: &'a crate::world_furniture_effect::FurnitureEffectQueue,
    pub light_grid: &'a mut crate::world_lighting::WorldLightGrid,
    pub particles: &'a mut crate::process_particles::ProcessParticleRuntime,
    pub executable: inf_formats::FrontendExecutable<'a>,
    pub birth_inputs: crate::process_particles::births::ParticleBirthInputs<'a>,
    pub load_factor: i32,
}

pub struct BriefingPickupOwners<'a> {
    pub mission_save: &'a mut inf_formats::MissionSaveState,
    pub current_world: &'a u32,
    pub text: Option<crate::grant_text::PickupTextData<'a>>,
}

pub struct BriefingWeaponFiringOwners<'a> {
    pub tables: &'a mut crate::object_runtime::ProcessActorBehaviorTables,
    pub sounds: &'a mut crate::process_sound::ProcessSoundRuntime,
    pub statistics: Option<&'a mut [u8; crate::world_mission::RECORD_0C_BYTES]>,
}

pub struct SoftwareBriefingFrame<'a, 'f, 'r> {
    pub scene: &'a mut SoftwareScene<'f, 'r>,
    pub resources: RenderResources<'r>,
    pub sine_quarter: &'a [u16; 4096],
    pub template_projection: SceneProjection,
    pub state: &'a mut FrontendRendererState,
    /// Shared process WORD 00533760, consumed by object callback 0043A270.
    pub selector_one: u16,
    pub world: BriefingWorld<'a>,
    pub effects: BriefingProcessEffects<'a>,
    /// Actual process settings DWORD +80, independent of pane visibility.
    pub dynamic_lighting: bool,
    pub object_light_policy: &'a mut dyn crate::object_runtime::ObjectLightPolicySource,
    pub object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy,
    pub object_draw_failure_policy: crate::object_runtime::Flag0ObjectDrawFailurePolicy,
    /// The world manager's byte +5C3, infrared vision, which the pane's
    /// terrain reads (0044AA10, 0044BC30).
    pub infrared: bool,
}

impl BriefingRendererError {
    fn feature(source: impl std::error::Error + 'static) -> Self {
        Self::Feature {
            source: Box::new(source),
        }
    }
}

impl SoftwareBriefingFrame<'_, '_, '_> {
    pub fn draw(
        &mut self,
        screen: &mut BriefingScreen,
        process: &FrontendProcessState,
        process_rng: &mut u32,
    ) -> Result<(), BriefingRendererError> {
        self.with_process_rng(process_rng, |owner| owner.draw_inner(screen, process))
    }

    /// Commit the already consumed draw prefix even if a later layer fails.
    fn with_process_rng<T>(
        &mut self,
        process_rng: &mut u32,
        draw: impl FnOnce(&mut Self) -> Result<T, BriefingRendererError>,
    ) -> Result<T, BriefingRendererError> {
        self.scene.set_process_rng(Some(*process_rng));
        let result = draw(self);
        if let Some(word) = self.scene.process_rng() {
            *process_rng = word;
        }
        // An unrelated later frontend draw must not inherit a stale copy.
        self.scene.set_process_rng(None);
        result
    }

    fn draw_inner(
        &mut self,
        screen: &mut BriefingScreen,
        process: &FrontendProcessState,
    ) -> Result<(), BriefingRendererError> {
        self.scene.replace_command_arena(0xc8000)?;
        self.begin(false)?;
        self.state.published_renderer = Some(self.scene.id());
        screen.draw()?;
        let runtime = screen
            .runtime()
            .ok_or(BriefingRendererError::MissingProgram)?;
        if !runtime.draw_enabled() {
            return Ok(());
        }
        let frame = BriefingSceneFrame {
            projection: SceneProjection {
                viewport: [
                    self.scene.pixels.frame.width() as i32,
                    self.scene.pixels.frame.height() as i32,
                ],
                ..self.template_projection
            },
            age_millis: runtime.screen_age_millis(),
            start_remaining_millis: runtime.start_remaining_millis(),
            animation: process.animation(),
            selector_one: self.selector_one,
        };
        // 44FFA0 reads current H14 flags and005471C0 through the retained
        // table even when this frame never reaches the pane's installer.
        self.scene
            .refresh_terrain_surface_globals(self.world.world_flags, frame.animation.value());
        let panels = [
            BriefingPanel::Map,
            BriefingPanel::Model,
            BriefingPanel::Terrain,
            BriefingPanel::Header,
            BriefingPanel::Text,
        ]
        .map(|panel| (panel, runtime.visibility(panel)));
        let model = runtime.model().copied();
        let model_age = runtime.model_age_millis();
        let terrain_inputs = BriefingTerrainInputs {
            frame,
            animation: panels[2].1,
            camera_position_words: runtime.camera_position_words(),
            heading: runtime.camera_heading(),
        };
        let terrain_effects = BriefingTerrainEffects {
            camera_disabled: runtime.terrain_camera_disabled(),
            pulse_remaining_micros: runtime.pulse_remaining_micros(),
            report_omissions: false,
        };
        let [camera_x, _, camera_z] = runtime.camera_position_words();
        let map_inputs = BriefingMapInputs {
            frame,
            animation: panels[0].1,
            map_scale_word: runtime.map_scale_word(),
            camera_xz_words: [camera_x, camera_z],
        };
        let math = MenuBasisMath::new(*self.sine_quarter);
        let report_omissions = screen.mark_draw_skips_logged();
        if report_omissions {
            eprintln!("briefing draw: complete terrain/world composition remains unowned (BRIEFING_RUNTIME.md)");
        }
        self.background(frame, &math)?;
        self.scene_panels(&panels[..2], model, model_age, map_inputs, &math, process)?;
        self.terrain_panel(
            terrain_inputs,
            BriefingTerrainEffects {
                report_omissions,
                ..terrain_effects
            },
            &math,
            process,
        )?;
        if terrain_effects.camera_disabled && panel_visible(terrain_inputs.animation) {
            screen
                .runtime_mut()
                .ok_or(BriefingRendererError::MissingProgram)?
                .camera_disabled_drawn();
        }
        let controls = screen
            .controls_mut()
            .ok_or(BriefingRendererError::MissingControls)?;
        self.menu_panel(panels[3], "root", &mut controls.root, frame, &math, process)?;
        self.menu_panel(panels[4], "text", &mut controls.text, frame, &math, process)?;
        self.decoration(frame, &math)?;
        self.tail(frame, screen.milestone_count(), process)?;
        Ok(())
    }

    fn begin(&mut self, reset_depth: bool) -> Result<(), SoftwareSceneError> {
        self.scene.submit(ResolvedSceneCommand::BeginBatch)?;
        if reset_depth {
            self.scene.reset_renderer();
        }
        Ok(())
    }

    fn background(
        &mut self,
        frame: BriefingSceneFrame,
        math: &MenuBasisMath,
    ) -> Result<(), BriefingRendererError> {
        // 0040B0F0 begins/resets around direct 004737A0 before any panels.
        self.begin(true)?;
        let entry = self.feature_entry()?;
        // Port widescreen: the backdrop disc keeps its 4:3 layout on the
        // centred canvas but draws in frame coordinates with the frame's
        // clip, so it runs on into the side strips instead of stopping at
        // the canvas edge.
        let canvas = self.scene.pixels.frame.canvas();
        if canvas.is_some() {
            self.scene.leave_canvas();
        }
        let result = self
            .resources
            .style(0x3b)
            .map_err(SoftwareSceneError::Resource)
            .and_then(|style| {
                for mut quad in briefing_background_quads(frame, style, math) {
                    if let Some(canvas) = canvas {
                        for corner in &mut quad.corners {
                            corner[0] = corner[0].wrapping_add(canvas.origin[0] as i16);
                            corner[1] = corner[1].wrapping_add(canvas.origin[1] as i16);
                        }
                    }
                    self.scene.draw_sprite_direct(quad)?;
                }
                self.scene.submit(ResolvedSceneCommand::FlushBatch)
            });
        if let Some(canvas) = canvas {
            self.scene.enter_canvas(canvas);
        }
        if let Err(error) = result {
            self.omit_feature("background", entry, error)?;
        }
        Ok(())
    }

    fn scene_panels(
        &mut self,
        panels: &[(BriefingPanel, BriefingPanelAnimation)],
        model: Option<BriefingModel>,
        model_age: i32,
        map_inputs: BriefingMapInputs,
        math: &MenuBasisMath,
        process: &FrontendProcessState,
    ) -> Result<(), BriefingRendererError> {
        let frame = map_inputs.frame;
        // 0040B0F0 brackets C390's map/frame and B840's preview/frame with
        // one 004A4400/004A4460 pair. Preserve their shared sort and command
        // arena; typed feature omission retains every reached sibling prefix.
        self.begin(true)?;
        // A visible map panel's frame runs 0049FEF0 just before the preview,
        // whose root then starts with that frame's context bases.
        let mut map_frame_drawn = false;
        for (panel, state) in panels[..2].iter().copied() {
            if !panel_visible(state) {
                continue;
            }
            if panel == BriefingPanel::Model {
                if let Some(model) = model {
                    let entry = self.feature_entry()?;
                    let result = model_extent(self.resources, model.model_slot)
                        .map_err(crate::scene_resources::SceneResourceError::Resource)
                        .and_then(|extent| {
                            draw_briefing_model_preview(
                                model,
                                model_age,
                                extent,
                                map_frame_drawn,
                                frame,
                                math,
                                &mut ResidentSceneHost {
                                    resources: self.resources,
                                    backend: &mut *self.scene,
                                },
                            )
                        });
                    if let Err(error) = result {
                        self.omit_feature("model preview", entry, error)?;
                    }
                }
            }
            // 0040C390 draws the map tiles and restores the full viewport
            // before 0040C685 submits the enclosing 00455080 frame.
            if panel == BriefingPanel::Map {
                self.map_backdrop(map_inputs, process)?;
                map_frame_drawn = true;
            }
            self.panel_frame(panel, state, frame, math)?;
        }
        // The next native operation opens the terrain batch; leave this root
        // closed so neither queue allocation nor failed replay is reset here.
        self.scene
            .submit(ResolvedSceneCommand::FlushBatch)
            .map_err(Into::into)
    }

    /// BB40's geometry branch uses the screen camera and preserves context
    /// fade while the optional backdrop temporarily publishes different globals.
    fn terrain_panel(
        &mut self,
        inputs: BriefingTerrainInputs,
        effects: BriefingTerrainEffects,
        math: &MenuBasisMath,
        process: &FrontendProcessState,
    ) -> Result<(), BriefingRendererError> {
        self.begin(true)?;
        if !panel_visible(inputs.animation) {
            return Ok(());
        }
        match briefing_terrain_pane(inputs, math) {
            Ok(Some(pane)) => {
                self.scene.pixels.pipeline.viewport = pane.clip_rect;
                if !effects.camera_disabled {
                    // The modern mode traces the camera's view of the world
                    // as it does gameplay's (crate::world_scene::feed_world).
                    let tracing = self.begin_pane_feed(pane, inputs.frame);
                    let drawn = self.terrain_backdrop(pane, inputs.frame).and_then(|()| {
                        self.terrain_ground(
                            pane,
                            inputs.frame,
                            effects.report_omissions,
                            process,
                            tracing,
                        )
                    });
                    if tracing {
                        self.scene.end_scene_feed();
                    }
                    drawn?;
                }
                let rows = if effects.camera_disabled {
                    BriefingNoiseRows::All
                } else {
                    BriefingNoiseRows::Random {
                        count: effects.pulse_remaining_micros / 2000,
                    }
                };
                let entry = self.feature_entry()?;
                if let Err(error) = self.terrain_static(pane.clip_rect, rows) {
                    self.omit_feature("camera static", entry, error)?;
                }
                // Native signed IDIV/truncation, then remainder modulo two.
                if effects.camera_disabled
                    && ((inputs.frame.animation.value() as i32) / 25) % 2 != 0
                {
                    let entry = self.feature_entry()?;
                    if let Err(error) = self.camera_disabled_text(pane, process) {
                        self.omit_feature("Camera disabled label", entry, error)?;
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                let entry = self.feature_entry()?;
                self.omit_feature("terrain pane setup", entry, error)?;
            }
        }
        let [width, height] = inputs.frame.projection.viewport;
        self.scene.pixels.pipeline.viewport = [
            0,
            0,
            width.wrapping_sub(1) as i16,
            height.wrapping_sub(1) as i16,
        ];
        self.panel_frame(BriefingPanel::Terrain, inputs.animation, inputs.frame, math)?;
        self.flush_feature("terrain frame").map_err(Into::into)
    }

    /// Begin the pane's scene feed where the frame's device traces rays:
    /// the pane's camera over the whole frame's native pixels, the world's
    /// ground, water, light and fog, and the pane's rectangle as the part
    /// the tracer fills. True when it does.
    fn begin_pane_feed(&mut self, pane: BriefingTerrainPane, frame: BriefingSceneFrame) -> bool {
        // The pane draws on the 4:3 canvas, which the port's widescreen
        // centres in a wider frame; the tracer works in the whole frame.
        let pixels = &self.scene.pixels.frame;
        let origin = pixels
            .canvas()
            .map_or([0, 0], |canvas| canvas.origin.map(|side| side as i16));
        let mut camera = crate::world_scene::scene_camera(pane.view, pane.context);
        camera.native = pixels.buffer_size().map(|side| side.max(1));
        camera.centre = [0, 1].map(|axis| camera.centre[axis] + f32::from(origin[axis]));
        let [left, top, right, bottom] = pane.clip_rect;
        let clip = [
            left + origin[0],
            top + origin[1],
            right + origin[0],
            bottom + origin[1],
        ];
        if !self.scene.begin_scene_feed(camera, false) {
            return false;
        }
        // The pane's own fog colour (0040BE7F); without it the backdrop
        // fails too, and the raster draws the pane alone.
        let Ok(far_color) = self.material_word(0x74) else {
            self.scene.end_scene_feed();
            return false;
        };
        crate::world_scene::feed_world(
            self.scene,
            self.resources,
            crate::world_scene::WorldFeedInputs {
                terrain: self.world.terrain,
                presentation: self.world.presentation,
                world_flags: self.world.world_flags,
                world: self
                    .effects
                    .pickups
                    .as_ref()
                    .map_or(0, |pickups| *pickups.current_world),
                eye: pane.view.eye,
                fade: pane.context.fade,
                far_color,
                infrared: self.infrared,
                // The briefing draws the world before its ticks.
                darkened: 0.0,
                // Retail's camera, which never goes under the water.
                eye_under: false,
                weather: crate::world_weather::WorldWeather::constant(),
                sparks: Vec::new(),
                clock: frame.animation.value(),
                clock_micros: frame.animation.accumulated_micros(),
                // The pane's camera follows no craft.
                craft: None,
                cloud_wake: Vec::new(),
                particles: self.effects.particles,
                executable: self.effects.executable,
            },
        );
        if let Some(builder) = self.scene.scene_feed_mut() {
            builder.feed.clip = Some(clip);
            // Retail draws no far terrain beyond the pane's fog: its haze
            // completes there, in the fog's colour.
            let fog = &mut builder.feed.fog;
            fog.far_ring = false;
            fog.far_color = fog.color;
        }
        true
    }

    fn terrain_ground(
        &mut self,
        pane: BriefingTerrainPane,
        frame: BriefingSceneFrame,
        report_omissions: bool,
        process: &FrontendProcessState,
        tracing: bool,
    ) -> Result<(), BriefingRendererError> {
        let tint = self.world.presentation.tint_word.to_le_bytes();
        let inputs = crate::terrain_draw::TerrainDrawInputs {
            view: pane.view,
            // AA10 republishes the retained pane fade, independently of
            // the optional backdrop's INT_MAX global fade override.
            context_projection: crate::model_context::ModelProjection {
                focal: pane.context.focal,
                viewport: pane.context.viewport,
                center: pane.context.center,
                fade: pane.context.fade,
            },
            footprint: crate::terrain_draw::TerrainFootprintWidth::for_surface(
                pane.context.viewport,
            ),
            coordinate_shift: self.scene.environment.coordinate_shift,
            tint: [tint[0], tint[1], tint[2]],
            lighting: crate::world_lighting::terrain_lighting_inputs(
                self.effects.light_grid,
                self.dynamic_lighting,
                self.effects.executable,
            )
            .map_err(BriefingRendererError::feature)?,
            clock: frame.animation.value(),
            world_flags: self.world.world_flags,
            night: self.infrared.then(|| {
                crate::terrain_draw::night_material(
                    crate::world_tint::INFRARED_RGB,
                    self.scene.pixels.renderer.material_shifts,
                )
            }),
            // Particle drawing retains its separate unowned service.
            particles_enabled: false,
            tables: self.scene.primitive_tables(),
        };
        let entry = self.feature_entry()?;
        match self
            .scene
            .draw_terrain(crate::software_scene::SoftwareTerrainDrawRequest {
                traversal: crate::terrain_draw::TerrainDrawRequest {
                    terrain: &mut *self.world.terrain,
                    inputs,
                },
                furniture: crate::software_scene::SoftwareFurnitureInputs {
                    state: &mut *self.effects.state,
                    pending: self.effects.pending,
                    light_grid: self.effects.light_grid,
                    particles: &mut *self.effects.particles,
                    executable: self.effects.executable,
                    birth_inputs: self.effects.birth_inputs,
                    load_factor: self.effects.load_factor,
                    crystal_flares: None,
                },
            }) {
            Ok(result) => {
                if report_omissions {
                    for omission in result.omissions {
                        eprintln!(
                            "briefing terrain: {:?}: {}",
                            omission.feature, omission.reason
                        );
                    }
                }
            }
            Err(error) => self.omit_feature("terrain geometry", entry, error)?,
        }
        // 0040C1E0 reaches the same 479500 owner as gameplay, after AA10
        // and before shared479570 object drawing and entity/terrain flush.
        match self.world.arena.run_flag0_light_pass(
            crate::object_runtime::Flag0LightPassRequest {
                grid: self.effects.light_grid,
                policy_source: self.object_light_policy,
                failure_policy: self.object_light_failure_policy,
                terrain: Some(&*self.world.terrain),
                rng: None,
                // The briefing draws the world before its ticks.
                world_delta: 0,
                sine_quarter: self.sine_quarter,
                lamps: None,
                lamps_light: false,
            },
        ) {
            Ok(outcome) if report_omissions => {
                for omission in outcome.omissions {
                    eprintln!("briefing object light {:?} skipped after reached writes: {:?}", omission.object, omission.reason);
                }
            }
            Ok(_) => {}
            Err(error) if report_omissions => eprintln!("briefing object light pass stopped after reached writes ({error}); remaining callbacks skipped"),
            Err(_) => {}
        }
        //0040C1EE: actual shared Flag0 manager, after light and before flush.
        //Pane context survives the backdrop's independent global fade writes.
        let mut context = crate::object_runtime::ObjectDrawContext {
            view: pane.view,
            near: pane.context.fade[0],
            far: pane.context.fade[1],
            far_color: self.scene.primitive_state.far_color,
        };
        // The pane is retail's view on the 4:3 canvas whatever the frame's
        // width, so its near callbacks (game state) keep retail's cone.
        let cone = crate::object_runtime::ObjectDrawCone::Retail;
        let update_byte = self.effects.object_draw_process.completion_53b890;
        let draw_result = self.world.arena.run_flag0_object_draw_pass(
            crate::object_runtime::Flag0ObjectDrawRequest {
                context: &mut context,
                cone,
                process: self.effects.object_draw_process,
                network_flag: &self.effects.birth_inputs.process.network_flag,
                terrain: &mut crate::world_actor_terrain::WorldActorTerrainHost {
                    residency: self.world.residency,
                    source: self.world.terrain_source,
                    prepared: self.world.terrain,
                },
                lighting: crate::object_runtime::ObjectDrawLighting {
                    grid: self.effects.light_grid,
                    tint_word: &mut self.world.presentation.tint_word,
                    direction: self.world.presentation.terrain_light_direction,
                    spacing: self.world.terrain.sampling().spacing,
                    executable: self.effects.executable,
                },
                host: &mut crate::software_scene::SoftwareObjectDrawHost {
                    terrain: crate::software_scene::ModelTerrainRead::Live(self.world.terrain),
                    effects: self.effects.firing.as_mut().map(|owners| {
                        crate::software_scene::SoftwareObjectEffectServices {
                            tables: Some(owners.tables),
                            // 0040C1EE draws pickups through the gameplay
                            // 0046DAF0, collection included.
                            pickup: self.effects.pickups.as_mut().map(|pickups| {
                                crate::software_scene::SoftwarePickupServices {
                                    text_data: pickups.text,
                                    terrain: self.world.terrain,
                                    mission_save: &mut *pickups.mission_save,
                                    current_world: pickups.current_world,
                                    load_factor: self.effects.load_factor,
                                }
                            }),
                            factory_process: self.effects.birth_inputs.process,
                            particles: self.effects.particles,
                            statistics: owners.statistics.as_deref_mut(),
                            sounds: Some(owners.sounds),
                            residency: self.world.residency,
                            phase: self.effects.birth_inputs.phase,
                            // The shared 00479570 runs source+10 vertices
                            // (00425870's legs among them) as in gameplay.
                            custom: Some(crate::software_scene::SoftwareCustomWorld {
                                load_factor: self.effects.load_factor,
                                terrain: &*self.world.terrain,
                                flags: self.world.world_flags,
                                update_byte,
                            }),
                            children: None,
                            pulse: None,
                            settings_7c: None,
                            settings_70: None,
                            network_initials: None,
                            network_team_flag: None,
                        }
                    }),
                    scene: self.scene,
                    animation: process,
                    executable: self.effects.executable,
                    attract_demo: self.effects.attract_demo,
                    failure_policy: self.object_draw_failure_policy,
                },
                failure_policy: self.object_draw_failure_policy,
            },
        );
        match draw_result {
            Ok(draw) if report_omissions => {
                for omission in draw.omissions {
                    eprintln!(
                        "briefing object draw {:?} omitted after reached writes: {:?}",
                        omission.object, omission.reason
                    );
                }
            }
            Ok(_) => {}
            Err(crate::object_runtime::Flag0ObjectDrawError::Host(error)) => {
                return Err(error.into())
            }
            Err(error) if report_omissions => {
                eprintln!("briefing object manager stopped after reached writes: {error}")
            }
            Err(_) => {}
        }
        // The tracer draws the pane's world now, its lights deposited; the
        // flush then replays only what the scene feed left to the raster.
        if tracing {
            crate::world_scene::feed_light_grid(
                self.scene,
                self.effects.light_grid,
                crate::world_scene::WorldLightFeed {
                    spacing: self.world.terrain.sampling().spacing.units(),
                    ground: self.dynamic_lighting,
                    terrain: self.world.terrain,
                    eye: pane.view.eye,
                    executable: self.effects.executable,
                },
            );
            self.scene.trace_scene_feed();
        }
        // 0040C201..0040C21C starts a new batch and invokes virtual+14(NULL)
        // after AA10/entity flush. Retain color and renderer policy, but
        // clear depth so the far static strips overlay the preceding world.
        self.scene.submit(ResolvedSceneCommand::FlushBatch)?;
        self.begin(true)?;
        Ok(())
    }

    fn terrain_static(
        &mut self,
        clip_rect: [i16; 4],
        rows: BriefingNoiseRows,
    ) -> Result<(), TerrainStaticDrawError> {
        let mut pass = BriefingNoisePass::new(clip_rect, rows);
        loop {
            let mut rng = self
                .scene
                .process_rng()
                .ok_or(TerrainStaticDrawError::MissingRng)?;
            let result = pass.next(&mut rng, |slot| {
                self.resources
                    .sprite_dimensions(i32::from(slot))
                    .map(|size| size[0])
            });
            self.scene.set_process_rng(Some(rng));
            let Some(quad) = result? else {
                return Ok(());
            };
            self.scene.draw_sprite_direct(quad)?;
        }
    }

    fn camera_disabled_text(
        &mut self,
        pane: BriefingTerrainPane,
        process: &FrontendProcessState,
    ) -> Result<(), StartTextError> {
        let font = self.resources.font(0)?;
        let measured = i32::from(measure_text_width(font, CAMERA_DISABLED_TEXT)? as i16);
        let origin = self.scene.primitive_state.projection.center;
        let mut request = LineTextRequest {
            text: CAMERA_DISABLED_TEXT,
            font,
            position_pixels: [
                pane.context.center[0].wrapping_sub(measured / 2),
                pane.context.center[1],
            ],
            color: MenuColor::from_word(self.resources.style(4)?),
            base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
            context_origin_pixels: origin,
        };
        let mut menu = SoftwareMenu::new(SoftwareMenuRequest {
            scene: &mut *self.scene,
            resources: self.resources,
            focal: self.template_projection.focal,
            process,
            state: &mut self.state.menu,
        })?;
        draw_line_text(&mut request, &self.resources, &mut menu)
            .map_err(SoftwareMenuError::Text)?;
        Ok(())
    }

    fn terrain_backdrop(
        &mut self,
        pane: BriefingTerrainPane,
        frame: BriefingSceneFrame,
    ) -> Result<(), BriefingRendererError> {
        self.scene.pixels.pipeline.viewport = pane.clip_rect;
        let presentation = *self.world.presentation;
        let rgb = presentation.tint_word.to_le_bytes();
        self.scene.submit(ResolvedSceneCommand::DirectionalLight {
            direction: presentation.terrain_light_direction,
            rgb: [rgb[0], rgb[1], rgb[2]],
        })?;
        self.scene.submit(ResolvedSceneCommand::AmbientLight(
            [rgb[0], rgb[1], rgb[2]].map(|value| i32::from(value >> 3)),
        ))?;
        // 0040BE6A ->004501A0 writes through this pane copy's inherited
        // renderer+50 alias. Later decoration sees the reached publication;
        // preview/panel template copies keep their own default callbacks.
        self.scene
            .install_terrain_surface_callbacks(self.world.world_flags, frame.animation.value());
        // 0040BE6F follows lighting publication and precedes the
        // backdrop/resources. It clears even with settings+80=0;
        // no 0045B180 anchor publication occurs in this caller.
        self.effects.light_grid.clear();
        self.scene
            .submit(ResolvedSceneCommand::Setup(pane.context))?;
        let far_color = self.material_word(0x74)?;
        self.scene
            .submit(ResolvedSceneCommand::FarColor(far_color))?;
        self.scene
            .submit(ResolvedSceneCommand::PublishView(pane.view))?;
        self.scene
            .submit(ResolvedSceneCommand::CoordinateShift(16))?;
        if self.resources.pool_capacity(RenderResourceKind::Model) >= 0x116 {
            // Resolve optional H8 before 0048EDA0 and the global fade override.
            let entry = self.feature_entry()?;
            let result = self
                .resources
                .resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Model,
                    argument: 0x116,
                })
                .map_err(crate::scene_resources::SceneResourceError::Resource)
                .and_then(|_| {
                    let mut host = ResidentSceneHost {
                        resources: self.resources,
                        backend: &mut *self.scene,
                    };
                    host.submit(SceneCommand::PublishFade(SceneFade::new([i32::MAX; 2])))?;
                    host.submit(SceneCommand::Model(
                        pane.backdrop(presentation.backdrop_height, frame),
                    ))
                });
            if let Err(error) = result {
                self.omit_feature("terrain backdrop", entry, error)?;
            }
        }
        // +84 is immediate. The backdrop, AA10 ground and Flag0 manager
        // share this batch until0040C1FC; no portable feature flush here.
        let entry = self.feature_entry()?;
        let fill = self
            .material_word(0x72)
            .and_then(|color| self.scene.draw_flat_quad_direct(pane.far_fill(color)));
        if let Err(error) = fill {
            self.omit_feature("terrain far fill", entry, error)?;
        }
        Ok(())
    }

    fn material_word(&self, argument: i32) -> Result<u32, SoftwareSceneError> {
        let resource = self.resources.resolve(RenderResourceRequest {
            kind: RenderResourceKind::Material,
            argument,
        })?;
        let RenderResourceData::Material(material) = resource.data else {
            unreachable!("typed H7 material")
        };
        Ok(self.scene.color_layout().convert_material(material))
    }

    /// C390 supplies the screen-owned current X/Z and scale to 471F70,
    /// clips all four direct +A4 tiles, then restores the full viewport.
    fn map_backdrop(
        &mut self,
        inputs: BriefingMapInputs,
        process: &FrontendProcessState,
    ) -> Result<(), BriefingRendererError> {
        if !panel_visible(inputs.animation) {
            return Ok(());
        }
        let entry = self.feature_entry()?;
        let result = self
            .resources
            .style(0x3b)
            .map_err(|error| BriefingRendererError::from(SoftwareSceneError::Resource(error)))
            .and_then(|style| {
                let plan = briefing_map_backdrop(inputs, style).expect("admitted map panel");
                self.scene.pixels.pipeline.viewport = plan.clip_rect;
                for tile in plan.tiles {
                    self.scene.draw_sprite_direct(tile.backdrop)?;
                    let marker_entry = self.feature_entry()?;
                    if let Err(error) =
                        self.map_markers(tile.markers, inputs.frame.age_millis, process)
                    {
                        self.omit_feature("map markers", marker_entry, error)?;
                    }
                }
                Ok(())
            });
        if let Err(error) = result {
            self.omit_feature("map backdrop", entry, error)?;
        }
        let [width, height] = inputs.frame.projection.viewport;
        self.scene.pixels.pipeline.viewport = [
            0,
            0,
            width.wrapping_sub(1) as i16,
            height.wrapping_sub(1) as i16,
        ];
        Ok(())
    }

    /// One native 00472D30 pass, streamed in G+48 order after its tile.
    /// Palette and style reads follow each actor's actual jitter RNG prefix.
    fn map_markers(
        &mut self,
        tile: BriefingMarkerTile,
        age_millis: i32,
        process: &FrontendProcessState,
    ) -> Result<(), MapMarkerDrawError> {
        let mut pass = BriefingMarkerPass::new(BriefingMarkerInputs {
            tile,
            scan_age: age_millis.wrapping_shl(2),
            jitter_radius: self.world.map_state.jitter_radius(),
            initial_jitter_x: None,
            initial_jitter_y: 0,
            network_active: self.world.network_active,
            material_shifts: self.scene.pixels.renderer.material_shifts,
            session_phase: 3,
        });
        for &id in self.world.arena.actor_list() {
            // Retained constructor prefixes remain in the actor FIFO, but
            // cannot submit a map marker or consume its jitter RNG.
            if self.world.arena.construction_is_stopped(id)? {
                continue;
            }
            let actor = BriefingMapActor::from_arena(self.world.arena, id)?;
            let mut rng = self
                .scene
                .process_rng()
                .ok_or(MapMarkerDrawError::MissingRng)?;
            let material = |lookup: BriefingMarkerMaterialLookup| {
                let material = |slot| {
                    let resource = self.resources.resolve(RenderResourceRequest {
                        kind: RenderResourceKind::Material,
                        argument: slot,
                    })?;
                    let RenderResourceData::Material(material) = resource.data else {
                        unreachable!("typed H7 material")
                    };
                    Ok::<_, RenderResourceError>(
                        self.scene.color_layout().convert_material(material),
                    )
                };
                let mut material_word = material(lookup.initial_slot)?;
                if let Some(slot) = lookup.replacement_slot {
                    material_word = material(slot)?;
                }
                Ok(material_word)
            };
            let style = |slot| self.resources.style(slot);
            let result = pass.actor(actor, &mut rng, material, style);
            // An error can follow a real random draw. Commit that prefix
            // before inspecting the result or invoking the pixel consumer.
            self.scene.set_process_rng(Some(rng));
            for quad in result? {
                self.scene.draw_sprite_direct(quad)?;
            }
        }
        self.map_annotations(tile, age_millis, process)
    }

    /// 473258 resolves font 0 then style 0 even when 48C6F0 finds no entries.
    /// Independent annotation list order follows the ordinary actors per tile.
    fn map_annotations(
        &mut self,
        tile: BriefingMarkerTile,
        age_millis: i32,
        process: &FrontendProcessState,
    ) -> Result<(), MapMarkerDrawError> {
        let resources = self.resources;
        let font = resources.font(0)?;
        resources.style(0)?;
        let inputs = BriefingAnnotationInputs {
            tile,
            scan_age: age_millis.wrapping_shl(2),
            radius_high_byte: 0,
        };
        for &id in self.world.arena.map_annotation_list() {
            let annotation = BriefingMapAnnotation::from_arena(self.world.arena, id)?;
            match briefing_map_annotation(annotation, inputs, || resources.style(0x24))? {
                BriefingAnnotationDraw::Radius(quad) => self.scene.draw_sprite_direct(quad)?,
                BriefingAnnotationDraw::Label {
                    label,
                    position_pixels,
                    color_word,
                } => {
                    // EXE0052F1D0 supplies "%d"; no localization/H0 lookup.
                    let text = label.to_string();
                    let mut request = LineTextRequest {
                        text: text.as_bytes(),
                        font,
                        position_pixels,
                        color: MenuColor::from_word(color_word),
                        base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
                        context_origin_pixels: self.scene.primitive_state.projection.center,
                    };
                    let mut menu = SoftwareMenu::new(SoftwareMenuRequest {
                        scene: &mut *self.scene,
                        resources,
                        focal: self.template_projection.focal,
                        process,
                        state: &mut self.state.menu,
                    })?;
                    draw_line_text(&mut request, &resources, &mut menu)
                        .map_err(SoftwareMenuError::Text)?;
                }
            }
        }
        Ok(())
    }

    fn panel_frame(
        &mut self,
        panel: BriefingPanel,
        state: BriefingPanelAnimation,
        frame: BriefingSceneFrame,
        math: &MenuBasisMath,
    ) -> Result<(), BriefingRendererError> {
        let entry = self.feature_entry()?;
        let result = draw_briefing_panel_frame(
            panel,
            state,
            frame,
            math,
            &mut ResidentSceneHost {
                resources: self.resources,
                backend: &mut *self.scene,
            },
        );
        if let Err(error) = result {
            self.omit_feature(&format!("{panel:?} frame"), entry, error)?;
        }
        Ok(())
    }

    fn menu_panel(
        &mut self,
        (panel, state): (BriefingPanel, BriefingPanelAnimation),
        label: &str,
        controller: &mut MenuStack,
        frame: BriefingSceneFrame,
        math: &MenuBasisMath,
        process: &FrontendProcessState,
    ) -> Result<(), BriefingRendererError> {
        // Header begins/resets even closed; text begins only when visible.
        if panel == BriefingPanel::Header || panel_visible(state) {
            self.begin(panel == BriefingPanel::Header)?;
            self.panel_frame(panel, state, frame, math)?;
            self.flush_feature(label)?;
        }
        if state.current_stage != 2 || state.requested_stage != 2 {
            return Ok(());
        }
        let entry = self.feature_entry()?;
        let result = SoftwareMenu::new(SoftwareMenuRequest {
            scene: &mut *self.scene,
            resources: self.resources,
            focal: self.template_projection.focal,
            process,
            state: &mut self.state.menu,
        })
        .map_err(MenuError::Host)
        .and_then(|mut menu| {
            draw_briefing_menu(
                controller,
                MenuDrawInput {
                    surface_pixels: frame.projection.viewport,
                    visibility: MenuVisibility::OwnerFlagSet,
                    sine_quarter: self.sine_quarter,
                },
                &mut menu,
                self.resources,
            )
        });
        match result {
            Ok(outcome) if !outcome.omitted_string_slots.is_empty() => eprintln!(
                "briefing draw: {label} H0 slots {:?} unavailable, skipping those rows",
                outcome.omitted_string_slots
            ),
            Ok(_) => {}
            Err(error) => {
                self.omit_feature(label, entry, error)?;
            }
        }
        Ok(())
    }

    fn flush_feature(&mut self, label: &str) -> Result<(), SoftwareSceneError> {
        self.scene.submit(ResolvedSceneCommand::FlushBatch).map_err(|error| {
            eprintln!("briefing draw: {label} pixel replay failed ({error}); retaining its stopped prefix");
            error
        })?;
        // Flush closes the queue root even on success. These frame/menu
        // callers need a root for following submissions; preserve depth and
        // graphics state. The map/model bracket closes without this restart.
        self.begin(false)
    }

    fn decoration(
        &mut self,
        frame: BriefingSceneFrame,
        math: &MenuBasisMath,
    ) -> Result<(), BriefingRendererError> {
        self.begin(true)?;
        let entry = self.feature_entry()?;
        let result = model_extent(self.resources, 98)
            .map_err(crate::scene_resources::SceneResourceError::Resource)
            .and_then(|extent| {
                let style = self.resources.style(5)?;
                let mut host = ResidentSceneHost {
                    resources: self.resources,
                    backend: &mut *self.scene,
                };
                draw_briefing_decoration(extent, style, frame, math, &mut host)?;
                host.submit(SceneCommand::FlushBatch)
            });
        if let Err(error) = result {
            self.omit_feature("decoration", entry, error)?;
        }
        Ok(())
    }

    fn tail(
        &mut self,
        frame: BriefingSceneFrame,
        milestone_count: Option<u32>,
        process: &FrontendProcessState,
    ) -> Result<(), BriefingRendererError> {
        if frame.start_remaining_millis != 0 {
            let entry = self.feature_entry()?;
            if let Err(error) = self.start_text(frame, process) {
                self.omit_feature("Start text", entry, error)?;
            }
            return Ok(());
        }
        let Some(count) = milestone_count else {
            eprintln!("briefing draw: H14 milestone count unavailable, skipping progress sprites");
            return Ok(());
        };
        let entry = self.feature_entry()?;
        let result = self
            .resources
            .style(5)
            .map_err(SoftwareSceneError::Resource)
            .and_then(|style| {
                for quad in briefing_progress_quads(frame, count, style, self.sine_quarter) {
                    self.scene.draw_sprite_direct(quad)?;
                }
                Ok(())
            });
        if let Err(error) = result {
            self.omit_feature("progress sprites", entry, error)?;
        }
        Ok(())
    }

    fn start_text(
        &mut self,
        frame: BriefingSceneFrame,
        process: &FrontendProcessState,
    ) -> Result<(), StartTextError> {
        // B0F0 uses the measured signed WORD, font0/style5 and raw flag2.
        // 004A3B20 supplies the shared base RGB; it adds no row shadow.
        let text = self.resources.string(0x166)?;
        let font = self.resources.font(0)?;
        let measured = i32::from(measure_text_width(font, text)? as i16);
        let [width, height] = frame.projection.viewport;
        let origin = self.scene.primitive_state.projection.center;
        let mut request = RawMenuTextRequest {
            text,
            font,
            position_pixels: [width / 2 - measured / 2, height / 2],
            width_pixels: width,
            color: MenuColor::from_word(self.resources.style(5)?),
            base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
            context_origin_pixels: origin,
            emission: MenuTextEmission::Draw,
            reveal: MenuTextReveal::Timed {
                age_millis: 2000i32.wrapping_sub(frame.start_remaining_millis as i32),
            },
        };
        SoftwareMenu::new(SoftwareMenuRequest {
            scene: &mut *self.scene,
            resources: self.resources,
            focal: self.template_projection.focal,
            process,
            state: &mut self.state.menu,
        })?
        .draw_raw_text(&mut request)?;
        Ok(())
    }
}

fn model_extent(resources: RenderResources<'_>, slot: i16) -> Result<u32, RenderResourceError> {
    let resource = resources.resolve(RenderResourceRequest {
        kind: RenderResourceKind::Model,
        argument: i32::from(slot),
    })?;
    let RenderResourceData::Model(model) = resource.data else {
        unreachable!("typed model lookup")
    };
    Ok(model.header.visibility_extent)
}

#[cfg(test)]
#[path = "briefing_renderer_tests.rs"]
mod tests;
