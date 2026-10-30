//! Live 0044CD00/0044CEF0 composition on the shared model queue.
//! Process allocations are borrowed for this phase, independently of renderer
//! epochs. Per-item failures retain sparse, particle, RNG and renderer writes.

use super::*;
use crate::{
    model_vertices::{
        compute_furniture_custom_vertex, FurnitureCustomTrail, FurnitureCustomVertexHost,
        ModelCustomVertexError, ModelCustomVertexRequest, ModelVertices,
    },
    process_particles::{
        births::{ParticleBirthError, ParticleBirthInputs, ParticleBirthRequest},
        ProcessParticleRuntime,
    },
    terrain_draw::TerrainFurnitureRequest,
    terrain_furniture::{
        draw::{
            draw_furniture_item, furniture_packed_value, FurnitureDrawContext, FurnitureDrawHost,
            FurnitureDrawOutcome, FurnitureDrawRequest, FurnitureDrawState, FurnitureModelOutcome,
            FurnitureModelRequest, FurnitureTrailRequest,
        },
        setup_furniture_item, walk_furniture_records, FurnitureAnchors, FurnitureSetupInputs,
    },
    terrain_runtime::TerrainSparseRecordId,
    world_furniture_effect::{resident::resident_placement_table, FurnitureEffectQueue},
    world_lighting::WorldLightGrid,
};

/// Actual process/world custody. The furniture consumer always reads the grid;
/// settings+80 only gates preceding light writers and the ground-strip policy.
pub struct SoftwareFurnitureInputs<'a> {
    pub state: &'a mut FurnitureDrawState,
    pub pending: &'a FurnitureEffectQueue,
    pub light_grid: &'a WorldLightGrid,
    pub particles: &'a mut ProcessParticleRuntime,
    pub executable: inf_formats::FrontendExecutable<'a>,
    pub birth_inputs: ParticleBirthInputs<'a>,
    pub load_factor: i32,
    /// The scenery crystals' flares (the world's; none elsewhere).
    pub crystal_flares: Option<&'a crate::crystal_flares::CrystalFlares>,
}

pub struct SoftwareTerrainDrawRequest<'a> {
    pub traversal: crate::terrain_draw::TerrainDrawRequest<'a>,
    pub furniture: SoftwareFurnitureInputs<'a>,
}

#[derive(Debug)]
pub struct FurnitureSceneOmission {
    pub record: TerrainSparseRecordId,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct FurnitureSceneReport {
    pub visited: usize,
    pub submitted_models: usize,
    pub no_draw: usize,
    pub omissions: Vec<FurnitureSceneOmission>,
}

pub(super) struct FurnitureModelCallbacks<'a> {
    pub terrain: &'a TerrainRuntime,
    pub request: FurnitureModelRequest,
    pub particles: &'a mut ProcessParticleRuntime,
    pub executable: inf_formats::FrontendExecutable<'a>,
    pub birth_inputs: ParticleBirthInputs<'a>,
    pub load_factor: i32,
}

impl FurnitureModelCallbacks<'_> {
    pub(super) fn packed_value(&self, selector: u32) -> Result<u16, SoftwareSceneServiceError> {
        if selector == 0 {
            return Ok(self.request.clock as u16);
        }
        if selector != 1 {
            return Ok(0);
        }
        // The record identity comes from the current H1 receiver. A preceding
        // missing-queue branch may already have cleared its actual bit8.
        let id = self.request.record;
        let record = self
            .terrain
            .buckets()
            .get(usize::from(id.bucket))
            .and_then(|bucket| bucket.records.get(id.index))
            .copied()
            .ok_or(SoftwareSceneServiceError::FurnitureRecord { record: id })?;
        Ok(furniture_packed_value(
            selector as i32,
            self.request.clock,
            record,
        ))
    }

    pub(super) fn custom_vertex(
        &mut self,
        request: ModelCustomVertexRequest<'_>,
        vertices: &mut ModelVertices,
        output: &mut Option<[i32; 3]>,
        rng: Option<&mut u32>,
    ) -> Result<(), ModelCustomVertexError<SoftwareSceneServiceError>> {
        compute_furniture_custom_vertex(
            request,
            vertices,
            output,
            &mut FurnitureVertexServices {
                callbacks: self,
                rng,
            },
        )
    }
}

struct FurnitureVertexServices<'s, 'a> {
    callbacks: &'s mut FurnitureModelCallbacks<'a>,
    rng: Option<&'s mut u32>,
}

impl FurnitureCustomVertexHost for FurnitureVertexServices<'_, '_> {
    type Error = SoftwareSceneServiceError;
    fn load_factor(&mut self) -> Result<i32, Self::Error> {
        Ok(self.callbacks.load_factor)
    }
    #[track_caller]
    fn random(&mut self) -> Result<u32, Self::Error> {
        let rng = self
            .rng
            .as_deref_mut()
            .ok_or(SoftwareSceneServiceError::FurnitureRng)?;
        Ok(crate::name_entry::name_entry_rng_next(rng))
    }
    fn birth_trail(&mut self, request: FurnitureCustomTrail) -> Result<(), Self::Error> {
        let rng = self
            .rng
            .as_deref_mut()
            .ok_or(SoftwareSceneServiceError::FurnitureRng)?;
        self.callbacks.particles.birth_trail(
            ParticleBirthRequest {
                particle_type: request.particle_type,
                position: request.position,
                velocity: Some(request.velocity),
                flags: request.flags,
                inputs: self.callbacks.birth_inputs,
            },
            self.callbacks.executable,
            rng,
        )?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub(super) enum SoftwareFurnitureError {
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Birth(#[from] ParticleBirthError<std::convert::Infallible>),
    #[error("furniture resource custody: {0}")]
    Custody(String),
    #[error("pending furniture fade dereferences null H8/{0}")]
    NullHeader(u16),
}

struct SceneFurnitureHost<'s, 'a, 'f, 'r> {
    scene: &'s mut SoftwareScene<'f, 'r>,
    particles: &'s mut ProcessParticleRuntime,
    executable: inf_formats::FrontendExecutable<'a>,
    birth_inputs: ParticleBirthInputs<'a>,
    load_factor: i32,
}

impl<'r> SceneFurnitureHost<'_, '_, '_, 'r> {
    fn model(&self, slot: u16) -> Result<Option<RenderResource<'r>>, SoftwareFurnitureError> {
        // 0049FEA0 uses the same inclusive pool bounds and genuine RET-null
        // lazy-loader policy as the visual/damage metadata owner.
        let resident = self.scene.resources.resident_runtime();
        if crate::object_runtime::resident_model_metadata(resident, slot)
            .map_err(|error| SoftwareFurnitureError::Custody(error.to_string()))?
            .is_none()
        {
            return Ok(None);
        }
        Ok(Some(self.scene.resources.resolve(
            RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: i32::from(slot),
            },
        )?))
    }
}

impl FurnitureDrawHost for SceneFurnitureHost<'_, '_, '_, '_> {
    type Error = SoftwareFurnitureError;
    fn bind_h1(&mut self, lookup: inf_formats::TerrainResourceLookup) -> Result<u16, Self::Error> {
        let table = resident_placement_table(self.scene.resources.resident_runtime())
            .map_err(|error| SoftwareFurnitureError::Custody(error.to_string()))?;
        table
            .slots
            .get(usize::from(lookup.raw()))
            .map(|slot| slot.resource_word)
            .ok_or_else(|| {
                SoftwareFurnitureError::Custody(format!(
                    "H11/0 lookup {} outside {} slots",
                    lookup.raw(),
                    table.slots.len()
                ))
            })
    }
    fn model_extent(&mut self, slot: u16) -> Result<u32, Self::Error> {
        let resource = self
            .model(slot)?
            .ok_or(SoftwareFurnitureError::NullHeader(slot))?;
        let RenderResourceData::Model(model) = resource.data else {
            unreachable!("typed H8")
        };
        Ok(model.header.visibility_extent)
    }
    fn far_color32(&mut self) -> Result<u32, Self::Error> {
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Material,
            argument: 32,
        };
        let RenderResourceData::Material(material) = self.scene.resources.resolve(request)?.data
        else {
            return Err(RenderResourceError::Registration { request }.into());
        };
        Ok(self.scene.color_layout.convert_material(material))
    }
    fn publish_fade(&mut self, context: &FurnitureDrawContext) -> Result<(), Self::Error> {
        self.scene
            .submit(ResolvedSceneCommand::FarColor(context.far_color))?;
        self.scene.submit(ResolvedSceneCommand::PublishFade(
            crate::frontend_scene::SceneFade::new(context.fade),
        ))?;
        Ok(())
    }
    fn birth_trail(
        &mut self,
        request: FurnitureTrailRequest,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        // 00489D90 runs where the next root's block lies; its stack is unproven.
        self.scene.clear_furniture_root_registers();
        self.particles.birth_trail(
            ParticleBirthRequest {
                particle_type: request.particle_type,
                position: request.position,
                velocity: Some(request.velocity),
                flags: request.flags,
                inputs: self.birth_inputs,
            },
            self.executable,
            rng,
        )?;
        Ok(())
    }
    fn draw_h1(
        &mut self,
        terrain: &TerrainRuntime,
        _: &mut FurnitureDrawContext,
        request: FurnitureModelRequest,
        rng: &mut u32,
    ) -> Result<FurnitureModelOutcome, Self::Error> {
        let Some(resource) = self.model(request.h1_slot)? else {
            return Ok(FurnitureModelOutcome::NoDraw);
        };
        let RenderResourceData::Model(model) = resource.data else {
            unreachable!("typed H8")
        };
        if model.header.command_word_count == 0 {
            return Ok(FurnitureModelOutcome::NoDraw);
        }
        let result = self.scene.execute_model_program(SoftwareModelDrawRequest {
            root_mode: crate::model_execution::ModelRootMode::CommonVisual,
            environment: self.scene.environment,
            argument: i32::from(request.h1_slot),
            context: ModelContext::new(request.position, request.basis),
            callbacks: ModelObjectCallbacks::Furniture(FurnitureModelCallbacks {
                terrain,
                request,
                particles: &mut *self.particles,
                executable: self.executable,
                birth_inputs: self.birth_inputs,
                load_factor: self.load_factor,
            }),
            terrain: ModelTerrainRead::Live(terrain),
            surface_table: ModelSurfaceTableOrigin::InheritedRenderer,
            primitive_context: ModelPrimitiveContext::Inherited,
            failure_policy: ModelFailurePolicy::StopScene,
            stack_registers: crate::model_execution::ModelStackRegisterPolicy::FurnitureRootCarry,
            rng: Some(rng),
        });
        if let Err(error) = &result {
            self.scene.latch_stop(error);
        }
        result?;
        Ok(FurnitureModelOutcome::Submitted)
    }
}

/// Keep renderer/queue/pool custody typed through the enclosing terrain owner.
enum FurnitureItemFailure {
    HardScene(SoftwareSceneError),
    HardCustody(Box<dyn std::error::Error>),
    Feature(String),
}
#[derive(Debug, Error)]
pub(super) enum FurniturePhaseError {
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error("{0}")]
    Feature(String),
    #[error("furniture source custody: {source}")]
    Custody {
        #[source]
        source: Box<dyn std::error::Error>,
    },
}

/// What a crystal catches in a traced frame (step 5): the ambient it glows by in its colour, and the lamp its facets
/// flash toward (the way to it and the light's colour).
struct CrystalCatch {
    ambient: [i32; 3],
    flash: Option<([f32; 3], [u8; 3])>,
}

/// Whether the ground stands over the line from eye-relative `from` to
/// `to`: a coarse test at seven points along it, so a crystal behind a
/// ridge stays dark.
fn ground_between(
    terrain: &crate::terrain_runtime::TerrainRuntime,
    eye: [f64; 3],
    from: [f32; 3],
    to: [f32; 3],
) -> bool {
    (1..8).any(|step| {
        let t = step as f32 / 8.0;
        let at: [f64; 3] =
            std::array::from_fn(|c| eye[c] + f64::from(from[c] + (to[c] - from[c]) * t));
        let ground = terrain.height_at([
            at[0].rem_euclid(65536.0) as u16,
            at[2].rem_euclid(65536.0) as u16,
        ]);
        f64::from(ground) > at[1] + 8.0
    })
}

impl SoftwareScene<'_, '_> {
    /// What the crystal furniture item at world `xyz` (its placement's
    /// `lookup`) catches in a traced frame: the headlamps' beams that reach
    /// it, and retail's lights at its cell (`light`: blasts, muzzle flashes).
    /// It glows in its world's colour (the world's light `tint` where the
    /// tuning names none), its facets flash toward the brightest lamp, and it
    /// lights its surroundings in its colour; `flare`, where a shot struck it
    /// (0 to 1, and what struck it), turns it and its light toward the
    /// flare's colour for that shot. A shot
    /// merely flying near it is no strike and makes it glow no more: the
    /// tracer lights its faces from the shot's light as it lights anything
    /// (2026-10-26: they light up only when actually struck). None for other furniture, outside a traced frame, or in
    /// the dark.
    fn crystal_catch(
        &mut self,
        lookup: inf_formats::TerrainResourceLookup,
        xyz: [i32; 3],
        light: [i8; 3],
        tint: [u8; 3],
        terrain: &crate::terrain_runtime::TerrainRuntime,
        flare: Option<(f32, crate::crystal_flares::StrikeKind)>,
    ) -> Option<CrystalCatch> {
        // Traced: the feed's camera and lamps; Vulkan's raster: its world's
        // camera, and only a struck crystal flares.
        let builder = self.scene_feed.as_ref();
        let eye = match builder {
            Some(builder) => builder.feed.camera.eye,
            None => self.pixels.frame.world_camera()?.eye,
        };
        let tuning = crate::modern_tuning::current();
        let crystals = tuning.crystals();
        let slot = resident_placement_table(self.resources.resident_runtime())
            .ok()?
            .slots
            .get(usize::from(lookup.raw()))?
            .resource_word;
        let resource = self
            .resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: i32::from(slot),
            })
            .ok()?;
        let RenderResourceData::Model(model) = resource.data else {
            return None;
        };
        let name = model
            .name
            .as_ref()
            .and_then(|name| resource.source.bytes.get(name.bytes.start..name.bytes.end))?;
        if !crystals.is_crystal(&String::from_utf8_lossy(name)) {
            return None;
        }
        let relative = |axis: usize| {
            ((f64::from(xyz[axis]) - eye[axis] + 32768.0).rem_euclid(65536.0) - 32768.0) as f32
        };
        // Its middle, and a point near its foot: a low beam often reaches
        // only a tall crystal's lower half.
        let height = (model.header.visibility_extent as f32).min(300.0);
        let centre = [relative(0), relative(1) + height * 0.5, relative(2)];
        let foot = [relative(0), relative(1) + height * 0.15, relative(2)];
        let looks = [false, true].map(|rear| tuning.headlight().look(rear));
        let mut lamps = 0.0_f32;
        let mut brightest: Option<([f32; 3], f32, [f32; 3])> = None;
        for lamp in builder.map_or(&[][..], |builder| &builder.feed.lamps[..]) {
            let beam = [centre, foot]
                .into_iter()
                .filter(|&point| !ground_between(terrain, eye, lamp.position, point))
                .map(|point| {
                    crate::world_scene::lamp_beam(lamp, &looks[usize::from(lamp.rear)], point)
                })
                .fold(0.0, f32::max);
            if beam <= 0.0 {
                continue;
            }
            lamps += beam;
            if brightest.map_or(true, |(_, most, _)| beam > most) {
                let toward: [f32; 3] = std::array::from_fn(|c| lamp.position[c] - centre[c]);
                let length =
                    (toward[0] * toward[0] + toward[1] * toward[1] + toward[2] * toward[2])
                        .sqrt()
                        .max(1.0e-3);
                brightest = Some((toward.map(|c| c / length), beam, lamp.color));
            }
        }
        let received = lamps + f32::from(light.iter().copied().max().unwrap_or(0).max(0)) / 128.0;
        let flaring = crystals.flare.zip(flare.filter(|(level, _)| *level > 0.0));
        if (received <= 0.01 || builder.is_none()) && flaring.is_none() {
            return None;
        }
        let colour = crystals
            .colors
            .get(&builder.map_or(u32::MAX, |builder| builder.feed.world))
            .copied()
            .unwrap_or_else(|| {
                let light = tint.map(f32::from);
                let most = light.iter().copied().fold(1.0, f32::max);
                light.map(|channel| channel / most)
            });
        // A little, easing toward the tuning's share as the light grows: the
        // user asked for a glow "a little bit".
        let glow = crystals.glow * (1.0 - (-received / 2.0).exp());
        // The crystal itself flares whole; the light it throws round it is
        // the flare's `under` share (2026-10-26: the underglow was
        // made a little tamer).
        let (colour, glow, thrown, reach) = match flaring {
            Some((look, (flare, kind))) => {
                let share = flare * flare;
                let tone = look.color_of(kind);
                (
                    std::array::from_fn(|c| colour[c] + (tone[c] - colour[c]) * share),
                    glow + look.strength * flare,
                    glow + look.strength * look.under.max(0.0) * flare,
                    crystals.reach + (look.reach - crystals.reach).max(0.0) * flare,
                )
            }
            None => (colour, glow, glow, crystals.reach),
        };
        let flash = brightest
            .filter(|_| lamps > 0.05)
            .map(|(toward, _, color)| {
                let amount = crystals.flash * (1.0 - (-lamps / 2.0).exp());
                (
                    toward,
                    color.map(|channel| (channel.clamp(0.0, 1.0) * amount * 255.0) as u8),
                )
            });
        let [r, g, b] = colour.map(|channel| (channel.clamp(0.0, 1.0) * 255.0) as u8);
        if let Some(builder) = self.scene_feed.as_mut() {
            builder.lone_glow(
                centre,
                crate::scene_feed::SceneGlow {
                    color: [r, g, b, 255],
                    strength: thrown,
                    reach,
                    ..Default::default()
                },
            );
        }
        Some(CrystalCatch {
            ambient: colour.map(|channel| (channel * glow * 128.0) as i32),
            flash,
        })
    }
}

pub(super) fn draw_furniture(
    scene: &mut SoftwareScene<'_, '_>,
    request: TerrainFurnitureRequest<'_>,
    inputs: &mut SoftwareFurnitureInputs<'_>,
    report: &mut FurnitureSceneReport,
) -> Result<(), FurniturePhaseError> {
    let mut rng = scene.process_rng().ok_or_else(|| {
        FurniturePhaseError::Feature("furniture requires the retained process RNG".into())
    })?;
    scene.clear_furniture_root_registers();
    let result = (|| {
        let hits = walk_furniture_records(request.terrain.buckets(), request.domain)
            .map_err(|error| FurniturePhaseError::Feature(error.to_string()))?;
        let outside_light_word = inputs
            .executable
            .word(0x0052_6068)
            .map_err(|error| FurniturePhaseError::Feature(error.to_string()))?;
        let anchors = inputs.light_grid.anchors();
        let mut context = FurnitureDrawContext {
            view: request.inputs.view,
            fade: request.inputs.context_projection.fade,
            far_color: request.far_color,
        };
        for hit in hits {
            // A crystal's flash takes the directional light over its draw.
            let mut flashed = None;
            let record = TerrainSparseRecordId {
                bucket: hit.bucket as u16,
                index: hit.index,
            };
            report.visited += 1;
            let item_result = (|| {
                let setup = setup_furniture_item(
                    hit.record,
                    hit.z,
                    &FurnitureSetupInputs {
                        cells: request.terrain.cells(),
                        height_shift: match request.terrain.sampling().height_scale {
                            crate::terrain_runtime::TerrainHeightScale::Units16 => 4,
                            crate::terrain_runtime::TerrainHeightScale::Units32 => 5,
                        },
                        mask: request.domain.mask,
                        anchors: FurnitureAnchors {
                            x: i32::from(anchors[0]),
                            z: i32::from(anchors[1]),
                        },
                        light_cells: inputs.light_grid.cells(),
                        outside_light_word,
                        tint_word: u32::from_le_bytes([
                            request.inputs.tint[0],
                            request.inputs.tint[1],
                            request.inputs.tint[2],
                            0,
                        ]),
                    },
                )
                .map_err(|error| FurnitureItemFailure::Feature(error.to_string()))?;
                let Some(item) = setup else {
                    return Ok(FurnitureDrawOutcome::LookupZero);
                };
                let tint = request.inputs.tint;
                let flare = inputs
                    .crystal_flares
                    .and_then(|flares| flares.flare([item.xyz[0] as u16, item.xyz[2] as u16]));
                let catch = scene.crystal_catch(
                    hit.record.resource_lookup,
                    item.xyz,
                    item.light_rgb,
                    [tint[0], tint[1], tint[2]],
                    &*request.terrain,
                    flare,
                );
                // A struck crystal's own faces, which take no light where
                // they are a half-transparent raster draw, laid again in its
                // spark, added, under Vulkan: the spark runs out from where
                // the bolt struck. A crystal the tracer draws takes the same
                // spark there (`pt_trace.comp`).
                let eye = scene
                    .scene_feed
                    .as_ref()
                    .map(|builder| builder.feed.camera.eye)
                    .or_else(|| scene.pixels.frame.world_camera().map(|camera| camera.eye));
                scene.draw_tint = crate::modern_tuning::current()
                    .crystals()
                    .flare
                    .zip(eye)
                    .zip(inputs.crystal_flares)
                    .and_then(|((look, eye), flares)| {
                        flares.spark([item.xyz[0] as u16, item.xyz[2] as u16], eye, &look)
                    });
                let ambient = catch.as_ref().map_or(item.ambient_rgb, |catch| {
                    std::array::from_fn(|c| item.ambient_rgb[c] + catch.ambient[c])
                });
                if let Some((direction, rgb)) = catch.and_then(|catch| catch.flash) {
                    flashed = Some((
                        scene.environment.lighting,
                        scene.primitive_state.effect_color,
                    ));
                    scene
                        .submit(ResolvedSceneCommand::DirectionalLight { direction, rgb })
                        .map_err(|error| FurnitureItemFailure::Feature(error.to_string()))?;
                }
                scene
                    .submit(ResolvedSceneCommand::AmbientLight(ambient))
                    .map_err(|error| {
                        if error.is_renderer_custody_fault() {
                            FurnitureItemFailure::HardScene(error)
                        } else {
                            FurnitureItemFailure::Feature(error.to_string())
                        }
                    })?;
                let pending = inputs.pending;
                let load_factor = inputs.load_factor;
                // Split the retained basis owner from the process producer host.
                let mut host = SceneFurnitureHost {
                    scene: &mut *scene,
                    particles: &mut *inputs.particles,
                    executable: inputs.executable,
                    birth_inputs: inputs.birth_inputs,
                    load_factor: inputs.load_factor,
                };
                draw_furniture_item(
                    FurnitureDrawRequest {
                        terrain: &mut *request.terrain,
                        record,
                        position: item.xyz,
                        context: &mut context,
                        pending,
                        load_factor,
                        clock: request.inputs.clock,
                        state: &mut *inputs.state,
                    },
                    &mut host,
                    &mut rng,
                )
                .map_err(|error| match error {
                    crate::terrain_furniture::draw::FurnitureDrawError::Host {
                        error: SoftwareFurnitureError::Scene(error),
                        ..
                    } if error.is_renderer_custody_fault() => {
                        FurnitureItemFailure::HardScene(error)
                    }
                    crate::terrain_furniture::draw::FurnitureDrawError::Host {
                        error: SoftwareFurnitureError::Resource(error),
                        ..
                    } if !matches!(
                        error,
                        RenderResourceError::NotResident { .. }
                            | RenderResourceError::SpriteVariant { .. }
                    ) =>
                    {
                        FurnitureItemFailure::HardScene(SoftwareSceneError::Resource(error))
                    }
                    crate::terrain_furniture::draw::FurnitureDrawError::Host {
                        error: error @ SoftwareFurnitureError::Custody(_),
                        ..
                    } => FurnitureItemFailure::HardCustody(Box::new(error)),
                    error => FurnitureItemFailure::Feature(format!("{error:?}")),
                })
            })();
            if let Some((lighting, effect)) = flashed {
                scene.environment.lighting = lighting;
                scene.primitive_state.effect_color = effect;
            }
            scene.draw_tint = None;
            match item_result {
                Ok(FurnitureDrawOutcome::Drawn(FurnitureModelOutcome::Submitted)) => {
                    report.submitted_models += 1
                }
                Ok(_) => report.no_draw += 1,
                Err(FurnitureItemFailure::HardScene(error)) => {
                    // Preserve the renderer failure without pool recovery.
                    // The closure still commits every reached process RNG draw.
                    // No omission receipt or registered-mark recovery occurs.
                    // Name the record: the terrain pass's own report only
                    // carries the error.
                    if error.native_error_code() != Some(0x301) {
                        eprintln!(
                            "terrain furniture {record:?} {:?} stopped the terrain pass: {error}",
                            hit.record
                        );
                    }
                    return Err(FurniturePhaseError::Scene(error));
                }
                Err(FurnitureItemFailure::HardCustody(source)) => {
                    scene.latch_stop(&*source);
                    return Err(FurniturePhaseError::Custody { source });
                }
                Err(FurnitureItemFailure::Feature(reason)) => {
                    // Log newly reached failures independently of the caller's
                    // first-frame summary. The renderer retains this receipt
                    // through frames and clears it at terrain epoch replacement.
                    if scene.furniture_omissions.insert((
                        record.bucket,
                        record.index,
                        reason.clone(),
                    )) {
                        eprintln!(
                            "terrain furniture {record:?} skipped after reached writes: {reason}"
                        );
                    }
                    report
                        .omissions
                        .push(FurnitureSceneOmission { record, reason });
                    scene
                        .continue_after_feature_error()
                        .map_err(FurniturePhaseError::Scene)?;
                }
            }
        }
        Ok(())
    })();
    scene.set_process_rng(Some(rng));
    result
}

#[cfg(test)]
mod tests;
