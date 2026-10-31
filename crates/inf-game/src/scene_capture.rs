//! The scene feed's model tap ([`crate::scene_feed`]): each primitive
//! instruction's faces copied into world space beside the game's own draw,
//! before projection, sorting and clipping, so the tracer gets every face,
//! backfaces and off-screen parts included. It reads only what the model
//! program already produced (the vertex slots' view points and the
//! instruction's materials) and changes nothing the game sees.

use inf_formats::{
    ModelEdge, ModelFanVertexAttributes, ModelInstructionKind, ModelMaterialDomain,
    ModelMaterialRef, ModelQuadLighting, ModelSlotRef, ModelTriangleLighting,
};

use crate::{
    d3d_textured::sprite_mode,
    model_context::ModelDispatch,
    model_execution::ModelPrimitiveRequest,
    model_primitives::{PrimitiveCallback, PrimitiveCommand},
    render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderResources,
    },
    scene_feed::{
        SceneBolt, SceneCamera, SceneEmitter, SceneFeedBuilder, SceneModelLight, SceneSurface,
        SceneVertex,
    },
};

/// H8 0x116, the sky's backdrop ring: it stays on the raster, which draws
/// it before the world; the tracer leaves the sky's pixels to it.
const BACKDROP_MODEL: u16 = 0x116;
/// Flat primitive modes the queue sorts as translucent (additive and the
/// other blended flats): those stay on the raster.
const TRANSLUCENT_FLAT_MODES: u32 = 0x16;
/// H3 variant flags the textured consumers blend with: additive (0x10),
/// darkening (0x20) and half alpha (0x08).
const TRANSLUCENT_SPRITE_FLAGS: u16 = 0x38;
const DARKENING_SPRITE_FLAG: u16 = 0x20;
/// Of those, only an additive sprite gives light.
const ADDITIVE_SPRITE_FLAG: u16 = 0x10;

/// The renderer state a primitive draws under, as the capture reads it:
/// the model context's mode and far colour, and the published fade.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CaptureState {
    pub mode: u32,
    pub far_color: u32,
    pub fade: [i32; 2],
}

/// Whether slot `index` (and its mirror twin, for a mirrored primitive) was
/// projected onto the ground: by opcode 2C, with its depth bias, or by 2B.
fn on_ground(request: &ModelPrimitiveRequest<'_>, index: i16, mirror: bool) -> bool {
    let vertices = request.vertices;
    let laid = |index: i16| vertices.depth_biased(index) || vertices.ground_projected(index);
    laid(index) || (mirror && laid(index ^ 1))
}

/// A face's corners' lists, inline up to eight corners (a fan may have more).
type Corners<T> = tinyvec::TinyVec<[T; 8]>;

/// A model's face as retail draws it ([`retail_face`]).
pub(crate) struct Face {
    pub(crate) slots: Corners<ModelSlotRef>,
    /// Each corner's shade slot, whose normal lights it.
    pub(crate) lighting: Corners<ModelSlotRef>,
    material: ModelMaterialRef,
    pub(crate) mirror: bool,
    /// Sprite-relative UVs per corner.
    pub(crate) uvs: Corners<[f32; 2]>,
    pub(crate) unlit: bool,
    /// Each corner's colour factors, 128 being one: a fan's own, the other
    /// faces' none.
    pub(crate) factors: Corners<[u8; 3]>,
}

/// A fan corner's colour factors (its record's lighting words): the raster
/// lights the corner its shade times them, 128 being one
/// (`model_primitives/fan.rs`). They paint most fans' colours, the red
/// cross's red and white among them.
fn fan_factors(attributes: &ModelFanVertexAttributes) -> [u8; 3] {
    let (ModelFanVertexAttributes::Untextured { lighting_and_end }
    | ModelFanVertexAttributes::Textured {
        lighting_and_end, ..
    }) = attributes;
    let [low, high] = *lighting_and_end;
    [low as u8, (low >> 8) as u8, high as u8]
}

/// `color` tinted by a corner's factors up to one, so the bounce light
/// takes the tint too.
pub(crate) fn tinted(color: [u8; 3], factors: [u8; 3]) -> [u8; 3] {
    std::array::from_fn(|channel| {
        ((u32::from(color[channel]) * u32::from(factors[channel].min(128))) >> 7) as u8
    })
}

/// `shade` brightened by a corner's factors past one; with [`tinted`], the
/// raster's colour times shade times factors.
fn brightened(shade: [u8; 3], factors: [u8; 3]) -> [u8; 3] {
    std::array::from_fn(|channel| {
        ((u32::from(shade[channel]) * u32::from(factors[channel].max(128))) >> 7).min(255) as u8
    })
}

/// A shade slot's world normal at unit length; zero when it has none.
pub(crate) fn unit(normal: [i32; 3]) -> [f32; 3] {
    let vector = normal.map(|value| value as f32);
    let length = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if length > 0.0 {
        vector.map(|value| value / length)
    } else {
        [0.0; 3]
    }
}

/// What Vulkan's raster does with a primitive of the sky's models while it
/// draws the modern sky itself (`vk_device/raster_sky.rs`): as the tracer
/// takes them ([`capture_primitive`]), the painting goes but for the land
/// its recipe names, drawn with its painted sky cut away, and the art it
/// keeps (stars, the Moon), drawn as retail drew it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RasterSkyDraw {
    /// Nobody draws it.
    Skip,
    /// The band's land, its painted sky cut away.
    Land(crate::modern_tuning::BandTuning),
    /// Art the modern sky keeps, as retail drew it.
    Kept,
}

/// How the raster draws `request` under the modern sky `look`, or none for
/// a primitive that is not the sky's.
pub(crate) fn raster_sky_draw(
    request: &ModelPrimitiveRequest<'_>,
    look: &crate::modern_tuning::SkyTuning,
) -> Option<RasterSkyDraw> {
    let model = request.model.identity.global_index;
    if model != BACKDROP_MODEL && !request.model.header.flags.forces_near_table() {
        return None;
    }
    let within = |range: Option<[i16; 2]>, id: i32| {
        range.is_some_and(|[first, last]| (i32::from(first)..=i32::from(last)).contains(&id))
    };
    let material = match &request.instruction.kind {
        ModelInstructionKind::Triangle(triangle) => triangle.material,
        ModelInstructionKind::Quad(quad) => quad.material,
        ModelInstructionKind::TriangleFan(fan) => fan.material,
        ModelInstructionKind::Opaque(command) if request.instruction.raw_command as u8 == 0x1b => {
            let kept = command
                .arguments
                .get(1)
                .is_some_and(|&sprite| within(look.objects, i32::from(sprite as i16)));
            return Some(if kept {
                RasterSkyDraw::Kept
            } else {
                RasterSkyDraw::Skip
            });
        }
        _ => return Some(RasterSkyDraw::Skip),
    };
    let id = i32::from(material.global_id);
    let textured = matches!(material.domain, ModelMaterialDomain::Handler3Texture);
    Some(if textured && within(look.band.sprites, id) {
        RasterSkyDraw::Land(look.band)
    } else if textured && within(look.objects, id) {
        RasterSkyDraw::Kept
    } else {
        RasterSkyDraw::Skip
    })
}

/// The face `request` draws as retail drew it, for a triangle, quad or fan:
/// its corners' slots and shade slots, material, sprite-relative UVs and
/// colour factors.
pub(crate) fn retail_face(request: &ModelPrimitiveRequest<'_>) -> Option<Face> {
    let raw = request.instruction.raw_command as u8;
    Some(match &request.instruction.kind {
        ModelInstructionKind::Triangle(triangle) => Face {
            slots: triangle.vertices.iter().copied().collect(),
            lighting: match &triangle.lighting {
                ModelTriangleLighting::Shared(slot) => std::iter::repeat_n(*slot, 3).collect(),
                ModelTriangleLighting::PerVertex(slots) => slots.iter().copied().collect(),
            },
            material: triangle.material,
            mirror: triangle.flags.emits_mirror_instance(),
            uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]].into_iter().collect(),
            unlit: matches!(raw, 0xd8 | 0xda),
            factors: std::iter::repeat_n([128; 3], 3).collect(),
        },
        ModelInstructionKind::Quad(quad) => Face {
            slots: quad.vertices.iter().copied().collect(),
            lighting: match &quad.lighting {
                ModelQuadLighting::Shared(slot) => std::iter::repeat_n(*slot, 4).collect(),
                ModelQuadLighting::PerVertex(slots) => slots.iter().copied().collect(),
            },
            material: quad.material,
            mirror: quad.flags.emits_mirror_instance(),
            uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
                .into_iter()
                .collect(),
            unlit: matches!(raw, 0xe0 | 0xe2),
            factors: std::iter::repeat_n([128; 3], 4).collect(),
        },
        ModelInstructionKind::TriangleFan(fan) => Face {
            slots: fan.vertices.iter().map(|vertex| vertex.vertex).collect(),
            lighting: fan
                .vertices
                .iter()
                .map(|vertex| vertex.lighting_ref)
                .collect(),
            material: fan.material,
            mirror: fan.flags.emits_mirror_instance(),
            uvs: fan
                .vertices
                .iter()
                .map(|vertex| match vertex.attributes {
                    ModelFanVertexAttributes::Textured { uv_words, .. } => {
                        uv_words.map(|word| f32::from(word) / 65536.0)
                    }
                    ModelFanVertexAttributes::Untextured { .. } => [0.0, 0.0],
                })
                .collect(),
            unlit: false,
            factors: fan
                .vertices
                .iter()
                .map(|vertex| fan_factors(&vertex.attributes))
                .collect(),
        },
        _ => return None,
    })
}

/// A blended face the raster keeps over the traced world: its corners join
/// the feed's faces, so a traced face the raster drew after it over the
/// same corners covers it ([`SceneFeedBuilder::raster_face`]).
fn note_raster_face(
    builder: &mut SceneFeedBuilder<'_>,
    request: &ModelPrimitiveRequest<'_>,
    face: &Face,
) {
    let passes: &[i16] = if face.mirror { &[1, 0] } else { &[0] };
    for &mirror in passes {
        let corners: Option<Vec<[f32; 3]>> = face
            .slots
            .iter()
            .map(|slot| {
                request
                    .vertices
                    .drawn_view((slot.raw() as i16) ^ mirror)
                    .map(|view| builder.world(view))
            })
            .collect();
        if let Some(corners) = corners {
            builder.raster_face(&corners);
        }
    }
}

/// Copy `request`'s faces into `builder`. True when the primitive is the
/// scene's: its faces went to the tracer, or deliberately to nobody (opcode
/// 2C's ground shadows), so in modern mode the raster leaves it. A
/// primitive the tap cannot place whole (a corner without a view point, a
/// missing texture) stays the raster's. The backdrop stays the raster's
/// too, and the feed keeps a copy as the sky reflections see.
pub(crate) fn capture_primitive<'a>(
    builder: &mut SceneFeedBuilder<'a>,
    resources: &RenderResources<'a>,
    request: &ModelPrimitiveRequest<'_>,
    state: CaptureState,
) -> bool {
    let CaptureState {
        mode,
        far_color,
        fade,
    } = state;
    // A draw that fogs its model over a range or in a colour of its own
    // (the actor flash) keeps that fog under the distance fog.
    builder.own_fog = builder
        .world_fog
        .is_some_and(|world| world != (fade, far_color));
    let model = request.model.identity.global_index;
    // The sky's models (H8 flag 0x20, force-near: the backdrop ring and what
    // some worlds hang on it, clouds, stars, mist) stay the raster's. Traced
    // as world geometry they stood as far walls: Level 1's cumulus ring hid
    // its painted mountains. The traced menu keeps its own way.
    let sky = model == BACKDROP_MODEL
        || (!builder.lit_by_trace && request.model.header.flags.forces_near_table());
    // Under the modern sky the painting goes, but for the land its recipe
    // names, traced as far ground with its painted sky cut away above each
    // column's skyline, and the additive art it keeps (stars, the Moon),
    // which the tracer adds to the sky. Nobody draws the rest of it.
    let modern = (sky && !builder.lit_by_trace && !builder.feed.infrared)
        .then(|| crate::modern_tuning::current().sky(builder.feed.world))
        .filter(|look| look.style == crate::modern_tuning::SkyStyle::Modern);
    let band = modern.map(|look| look.band);
    let within = |range: Option<[i16; 2]>, id: i32| {
        range.is_some_and(|[first, last]| (i32::from(first)..=i32::from(last)).contains(&id))
    };
    let land = |id: i32| within(band.and_then(|band| band.sprites), id);
    let object = |id: i32| within(modern.and_then(|look| look.objects), id);
    if let Some(plane) = builder.water_plane {
        let height = builder.world(request.transform.view_position)[1];
        if blast_wave_under_water(model_name(request), height, plane) {
            return true;
        }
    }
    let raw = request.instruction.raw_command as u8;
    if band.is_some() {
        match &request.instruction.kind {
            ModelInstructionKind::Triangle(_)
            | ModelInstructionKind::Quad(_)
            | ModelInstructionKind::TriangleFan(_) => {}
            // A billboard the recipe keeps (the Moon) waits for its command's
            // corners (scene_feed's place_sky_billboard); the raster's own
            // sky draws lie under the modern sky.
            ModelInstructionKind::Opaque(command) if matches!(raw, 0x1b) => {
                let Some(&sprite) = command.arguments.get(1) else {
                    return true;
                };
                let id = sprite as i16;
                if !object(i32::from(id)) {
                    return true;
                }
                if let Ok(resource) = resources.resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument: i32::from(id),
                }) {
                    if let RenderResourceData::Sprite(entry) = resource.data {
                        let masked = entry
                            .selectors
                            .variant_index(sprite_mode(mode))
                            .and_then(|selector| entry.variants.get(selector))
                            .map(|variant| variant.flags.raw() & 1 != 0);
                        if let Some(masked) = masked {
                            let index = builder.sprite(id, resource, sprite_mode(mode));
                            builder.sky_billboard = Some((index, model, masked));
                        }
                    }
                }
                return true;
            }
            _ => return true,
        }
    }
    let face = match &request.instruction.kind {
        ModelInstructionKind::Edge(edge) => {
            return capture_edge(builder, resources, request, edge, mode, far_color)
        }
        // The billboards (1A, 1B) exist only in screen space; a glowing
        // model's (the token pickups') still light the world.
        ModelInstructionKind::Opaque(command) if matches!(raw, 0x1a | 0x1b) => {
            // A gate's flare (1B) is redrawn as a glint where its quad lands.
            if raw == 0x1b && gate_flare(builder, resources, request, &command.arguments, mode) {
                return true;
            }
            glow_billboard(builder, resources, request, &command.arguments, mode);
            // An additive one sunk under the water waits for its command's
            // corners to become the tracer's glow (scene_feed's
            // place_sunk_billboard): the raster's blend would fail the
            // traced water's depth and nothing would show (a token on a
            // lake's bed).
            sink_billboard(builder, resources, request, &command.arguments, mode);
            return false;
        }
        kind => {
            let Some(mut face) = retail_face(request) else {
                return false;
            };
            if matches!(kind, ModelInstructionKind::Quad(_)) {
                unfold_duct(model_name(request), &mut face.slots, &mut face.lighting);
            }
            face
        }
    };
    if face.slots.len() < 3 {
        return false;
    }
    let id = i32::from(face.material.global_id);
    let textured = matches!(face.material.domain, ModelMaterialDomain::Handler3Texture);
    let kept = textured && object(id);
    if band.is_some() && !(textured && (land(id) || kept)) {
        return true;
    }
    // The band is land; kept art stays sky, added by the tracer.
    let band = band.filter(|_| !kept);
    // The material: a sprite (resolved at the trace) or a flat colour, and
    // whether the sprite is masked (variant flag 1: its index 0 or word 0
    // texels upload transparent, d3d_texture::upload).
    // An additive sprite sunk under the water is fed to the tracer as its
    // glow (see `sunk` below); every other blended one stays the raster's.
    let mut sunk = false;
    let mut additive_glow = false;
    let (texture, color, cutout) = match face.material.domain {
        ModelMaterialDomain::Handler3Texture => {
            let id = face.material.global_id;
            if id < 0 || id as usize > resources.pool_capacity(RenderResourceKind::Sprite) {
                return false;
            }
            let Ok(resource) = resources.resolve(RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument: i32::from(id),
            }) else {
                return false;
            };
            let RenderResourceData::Sprite(entry) = resource.data else {
                return false;
            };
            // A variant selector of none draws nothing, here as in the raster.
            let Some(selector) = entry.selectors.variant_index(sprite_mode(mode)) else {
                return true;
            };
            let Some(variant) = entry.variants.get(selector) else {
                return false;
            };
            // A darkening sprite laid on the ground is a model's built-in
            // ground shadow (the vehicles' under 2C's depth bias, round blobs
            // under 2B): the tracer's own shadows replace it, so nobody
            // draws it.
            if !sky
                && variant.flags.raw() & DARKENING_SPRITE_FLAG != 0
                && face
                    .slots
                    .iter()
                    .any(|slot| on_ground(request, slot.raw() as i16, face.mirror))
            {
                return true;
            }
            // Additive, darkening and half-alpha sprites stay blended on the
            // raster, over the tracer's world; a glowing model's additive
            // ones (the crystals') still light it, and every additive model
            // face (flamethrower cones, turbo jet flames, blast waves,
            // crystal shells) also enters the tracer's glow acceleration
            // structure so reflections see its 3D geometry.
            if !sky && variant.flags.raw() & TRANSLUCENT_SPRITE_FLAGS != 0 {
                if variant.flags.raw() & ADDITIVE_SPRITE_FLAG != 0 {
                    glow_face(builder, request, &face, id, resource, mode);
                    additive_glow = true;
                }
                // The tracer's water is depth in front of the raster's
                // overlay: an additive sprite under it would fail that test
                // and vanish (a crystal on a lake's bed). Such a face goes to
                // the tracer as its glow instead, which the water adds below
                // it as retail's blend adds it, and it still lights the
                // water; its raster draw is skipped with the primitive. Above
                // the water the blend stays retail's.
                sunk = additive_glow && under_water(builder, request, &face);
                if !additive_glow {
                    note_raster_face(builder, request, &face);
                    return false;
                }
            }
            let sprite = builder.sprite(id, resource, sprite_mode(mode));
            if let Some(band) = band {
                builder.cut_band(sprite, band);
            }
            (
                Some(sprite),
                [255, 255, 255],
                band.is_some() || variant.flags.raw() & 1 != 0,
            )
        }
        ModelMaterialDomain::Handler7Material => {
            if mode & TRANSLUCENT_FLAT_MODES != 0 {
                return false;
            }
            let Ok(resource) = resources.resolve(RenderResourceRequest {
                kind: RenderResourceKind::Material,
                argument: i32::from(face.material.global_id),
            }) else {
                return false;
            };
            let RenderResourceData::Material(value) = resource.data else {
                return false;
            };
            (None, builder.material_rgb(value.raw()), false)
        }
    };
    let surface = if sunk || additive_glow {
        SceneSurface::Glow
    } else if band.is_some() {
        SceneSurface::Shown
    } else if sky {
        SceneSurface::Sky
    } else if face.unlit {
        SceneSurface::Emissive
    } else {
        SceneSurface::Opaque
    };
    // The Far dispatch's records fog each corner by its projection's fog
    // byte towards the far colour; Near records are unfogged.
    let far = (request.dispatch == ModelDispatch::Far)
        .then(|| crate::d3d_effect::far_rgb(far_color, builder.material_shifts));
    let glows_with_thrust = (surface == SceneSurface::Opaque
        && matches!(face.material.domain, ModelMaterialDomain::Handler3Texture))
    .then(|| thrust_glow(builder, request, face.material.global_id))
    .flatten();
    let first_triangle = builder.feed.triangles.len();
    // Each pass's corners first, so a primitive is taken whole or not at all.
    let passes: &[i16] = if face.mirror { &[1, 0] } else { &[0] };
    let mut polygons: tinyvec::ArrayVec<
        [(Corners<SceneVertex>, Corners<Option<[i32; 3]>>, i16); 2],
    > = tinyvec::ArrayVec::new();
    for &mirror in passes {
        let mut corners: Corners<SceneVertex> = Corners::new();
        // Each corner's world normal as retail lit it, for a round part's
        // new corners.
        let mut normals: Corners<Option<[i32; 3]>> = Corners::new();
        let mut shadow = false;
        for (((slot, uv), light), &factors) in face
            .slots
            .iter()
            .zip(&face.uvs)
            .zip(&face.lighting)
            .zip(&face.factors)
        {
            let index = (slot.raw() as i16) ^ mirror;
            if request.vertices.depth_biased(index) {
                shadow = true;
                break;
            }
            let Some(view) = request.vertices.drawn_view(index) else {
                return false;
            };
            let shade_slot = i32::from((light.raw() as i16) ^ mirror);
            let normal = if builder.lit_by_trace {
                // The traced menu's planets are spheres, lit by the tracer:
                // each corner faces out from the model's middle, smooth
                // however few its faces.
                let middle = builder.world(request.transform.view_position);
                let corner = builder.world(view);
                let out: [f32; 3] = std::array::from_fn(|axis| corner[axis] - middle[axis]);
                let length = out.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
                if length > 0.0 {
                    out.map(|axis| axis / length)
                } else {
                    [0.0; 3]
                }
            } else {
                request.shades.normal(shade_slot).map_or([0.0; 3], unit)
            };
            // The shade the producers read: the shared arena's, which a slot
            // the model never lit keeps from an earlier model, as in retail.
            let shade = request
                .shades
                .scratch_shade(shade_slot)
                .ok()
                .and_then(|shade| shade.rgb)
                .filter(|_| !builder.lit_by_trace)
                .map_or([0; 4], |rgb| {
                    let [r, g, b] = brightened(rgb, factors);
                    [r, g, b, 255]
                });
            let fog = far
                .and_then(|[r, g, b]| {
                    crate::model_primitives::vertex_fog::<()>(request, usize::from(index as u16))
                        .ok()
                        .map(|amount| [r, g, b, amount])
                })
                .unwrap_or([0; 4]);
            let [r, g, b] = tinted(color, factors);
            let position = builder.world(view);
            let motion = builder.vertex_motion(
                request.model.identity.global_index,
                request.transform,
                index as u16,
                position,
            );
            corners.push(SceneVertex {
                position,
                normal,
                uv: *uv,
                color: [r, g, b, 255],
                shade,
                fog,
                motion,
            });
            normals.push(request.shades.normal(shade_slot));
        }
        // Opcode 2C's ground shadows give way to the tracer's own.
        if !shadow {
            polygons.push((corners, normals, mirror));
        }
    }
    // The light retail shaded this model with, for the tracer to shade it
    // the same way and shadow its sun.
    let light = match request.shades.lighting() {
        Some(lighting) if !builder.lit_by_trace => {
            Some(builder.model_light(SceneModelLight::from_model(lighting)))
        }
        _ => None,
    };
    name_model(builder, request);
    trail_exhausts(builder, request);
    // A wheel's corners, about its hub (its model's origin): where the
    // craft's wheels stand, for the puddles they stir.
    if is_wheel(model_name(request)) {
        let hub = builder.world(request.transform.view_position);
        for (corners, _, _) in &polygons {
            for corner in corners {
                builder.wheel_corner(hub, corner.position);
            }
        }
    }
    let round = round_draw(builder, resources, request, mode);
    let lighting = request.shades.lighting().filter(|_| !builder.lit_by_trace);
    for (corners, normals, mirror) in &polygons {
        let mirror = *mirror;
        if let Some(round) = &round {
            let tuning = crate::modern_tuning::current();
            let (tolerance, most) = (tuning.round().tolerance, tuning.round().most);
            let pixels = round.pixels;
            let count = |turn: f32, distance: f32| {
                crate::round_parts::pieces(turn, distance * pixels, tolerance, most)
            };
            let positions: Corners<[f32; 3]> =
                corners.iter().map(|corner| corner.position).collect();
            let units: Corners<[f32; 3]> = corners.iter().map(|corner| corner.normal).collect();
            // The treads' lugs: the corners on a ring the treads use, by
            // their slots in this pass.
            let on_ring: Corners<bool> = face
                .slots
                .iter()
                .map(|slot| {
                    let slot = ((slot.raw() as i16) ^ mirror) as u16;
                    round.model.tread_slots.binary_search(&slot).is_ok()
                })
                .collect();
            let u: Corners<f32> = face.uvs.iter().map(|uv| uv[0]).collect();
            let treads = &tuning.round().treads;
            let height = |u: f32| crate::round_parts::tread_height(&round.profile, u);
            let lugs = crate::round_parts::Lugs {
                on_ring: &on_ring,
                u: &u,
                height: &height,
                depth: treads.depth,
                cuts: (treads.lugs * treads.per_lug) as f32,
                pixels,
                from: treads.from,
            };
            let lugs = (treads.depth > 0.0 && !round.profile.is_empty()).then_some(&lugs);
            let frame = &round.frames[usize::from(mirror != 0)];
            let split = if round.sphere {
                crate::round_parts::split_sphere(
                    frame.centre(),
                    &positions,
                    pixels,
                    tolerance,
                    most,
                )
            } else {
                crate::round_parts::split_face(frame, &positions, &units, &count, lugs)
            };
            if let Some(split) = split {
                // The split's corners, in a list kept from draw to draw.
                let mut vertices = std::mem::take(&mut builder.split_corners);
                vertices.clear();
                vertices.extend(
                    split.corners.iter().map(|corner| {
                        round_vertex(corner, corners, normals, &face.factors, lighting)
                    }),
                );
                builder.split_face(
                    corners,
                    &vertices,
                    &split.triangles,
                    texture,
                    surface,
                    Some(model),
                    light,
                    cutout,
                );
                builder.split_corners = vertices;
                continue;
            }
        }
        builder.face(corners, texture, surface, Some(model), light, cutout);
    }
    if let Some(glow) = glows_with_thrust {
        let position = builder.world(request.transform.view_position);
        builder
            .feed
            .thrust_glows
            .push(crate::scene_feed::SceneThrustGlow {
                triangles: first_triangle..builder.feed.triangles.len(),
                position,
                gain: glow.gain,
                seconds: glow.seconds,
            });
    }
    if additive_glow && !sunk {
        builder.glow_fed = true;
        return false;
    }
    !sky || band.is_some() || kept
}

/// Whether `face` lies under the water's plane: the tracer's water then hides
/// it, so its sprite is fed as the tracer's own emissive
/// ([`model_face`]'s `sunk`). False for a world without water, and for a
/// face whose corners the draw cannot place.
fn under_water(
    builder: &SceneFeedBuilder<'_>,
    request: &ModelPrimitiveRequest<'_>,
    face: &Face,
) -> bool {
    let mut points: Corners<[f32; 3]> = Corners::new();
    for slot in &face.slots {
        let Some(view) = request.vertices.drawn_view(slot.raw() as i16) else {
            return false;
        };
        points.push(builder.world(view));
    }
    sunk_below(builder.water_plane, &points)
}

/// Whether a face whose corners are `points` (eye-relative, as the feed
/// takes them) lies under the water's `plane`, eye-relative too, seen from
/// above it: a unit of clearance keeps a face at the surface out of it,
/// where the raster's blend still shows, and with the eye under the water
/// no traced surface stands in front of the raster's blend.
fn sunk_below(plane: Option<f32>, points: &[[f32; 3]]) -> bool {
    let Some(plane) = plane.filter(|plane| *plane < 0.0) else {
        return false;
    };
    !points.is_empty()
        && points.iter().map(|point| point[1]).sum::<f32>() / (points.len() as f32) < plane - 1.0
}

/// A round part in one draw (the tuning's `round`): its frame, eye-relative, as plain and as mirrored faces take it
/// (the centre's x negated), the pixels a unit of it takes on screen at its
/// depth, the slots its treads use and their sprite's brightness.
struct RoundDraw {
    frames: [crate::round_parts::PartFrame; 2],
    /// A sphere round the frames' centre ([`crate::round_parts::split_sphere`]).
    sphere: bool,
    pixels: f32,
    model: std::rc::Rc<crate::round_parts::RoundModel>,
    profile: std::rc::Rc<[f32]>,
}

/// `request`'s model as a round part in this draw: none for a model the
/// tuning does not name, one behind the eye, or in the traced menu (its
/// planets) one that is not a sphere.
fn round_draw<'a>(
    builder: &mut SceneFeedBuilder<'a>,
    resources: &RenderResources<'a>,
    request: &ModelPrimitiveRequest<'_>,
    mode: u32,
) -> Option<RoundDraw> {
    let model = request.model.identity.global_index;
    let round = match builder.round_parts.get(&model) {
        Some(round) => round.clone(),
        None => {
            let tuning = crate::modern_tuning::current();
            let round = model_name(request).and_then(|name| {
                let part = tuning.round().part(&String::from_utf8_lossy(name))?;
                Some(std::rc::Rc::new(crate::round_parts::RoundModel::new(
                    part.clone(),
                    &request.model.program,
                    &tuning.round().treads.sprites,
                )))
            });
            builder.round_parts.insert(model, round.clone());
            round
        }
    }?;
    let part = &round.part;
    let sphere = part.axis == crate::modern_tuning::RoundAxis::Sphere;
    let depth = request.transform.view_position[2] as f32;
    if depth <= 0.0 || builder.lit_by_trace && !sphere {
        return None;
    }
    let first = match part.axis {
        crate::modern_tuning::RoundAxis::X => 1,
        // A sphere's frame gives only its centre.
        crate::modern_tuning::RoundAxis::Y | crate::modern_tuning::RoundAxis::Sphere => 4,
        crate::modern_tuning::RoundAxis::Z => 7,
    };
    let words = request.transform.view_basis.words;
    let row = |first: usize| -> [f32; 3] {
        let row: [f32; 3] = std::array::from_fn(|k| f32::from_bits(words[first + k]));
        builder.world(row)
    };
    let [x, y, z] = [row(1), row(4), row(7)];
    let axis = row(first);
    let origin = builder.world(request.transform.view_position);
    let [cx, cy, cz] = part.centre;
    let frame = |cx: f32| {
        crate::round_parts::PartFrame::new(
            std::array::from_fn(|k| origin[k] + x[k] * cx + y[k] * cy + z[k] * cz),
            axis,
        )
        .map(|frame| frame.within(part.radius, part.along))
    };
    let frames = [frame(cx)?, frame(-cx)?];
    let camera = &builder.feed.camera;
    let native = camera.native[1].max(1) as f32;
    let shown = if builder.target_height > 0.0 {
        builder.target_height
    } else {
        native
    };
    let pixels = camera.focal[1] * shown / native / depth;
    let profile = round
        .tread_sprite
        .and_then(|sprite| {
            builder
                .tread_profiles
                .entry(sprite)
                .or_insert_with(|| tread_profile(resources, sprite, mode).map(Into::into))
                .clone()
        })
        .unwrap_or_else(|| std::rc::Rc::from([]));
    Some(RoundDraw {
        frames,
        sphere,
        pixels,
        model: round,
        profile,
    })
}

/// Tread sprite `id`'s brightness across it, column by column, from its
/// darkest (0) to its brightest (1), as the raster samples it in `mode`.
fn tread_profile(resources: &RenderResources<'_>, id: i16, mode: u32) -> Option<Vec<f32>> {
    let resource = resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: i32::from(id),
        })
        .ok()?;
    let RenderResourceData::Sprite(entry) = resource.data else {
        return None;
    };
    let variant = entry.variant(sprite_mode(mode))?;
    let texels = variant
        .decode_rgba(resource.source.bytes, inf_formats::SpriteShade::Base)
        .ok()?;
    let [width, height] = [usize::from(variant.width), usize::from(variant.height)];
    if width == 0 || texels.len() != width * height {
        return None;
    }
    let columns: Vec<f32> = (0..width)
        .map(|x| {
            let (sum, count) = (0..height)
                .map(|y| texels[y * width + x])
                .filter(|texel| texel.alpha > 0)
                .fold((0.0, 0.0), |(sum, count), texel| {
                    let bright =
                        (f32::from(texel.red) + f32::from(texel.green) + f32::from(texel.blue))
                            / 3.0;
                    (sum + bright, count + 1.0)
                });
            if count > 0.0 {
                sum / count
            } else {
                0.0
            }
        })
        .collect();
    let low = columns.iter().copied().fold(f32::MAX, f32::min);
    let high = columns.iter().copied().fold(f32::MIN, f32::max);
    (high > low).then(|| {
        columns
            .iter()
            .map(|value| (value - low) / (high - low))
            .collect()
    })
}

/// A corner of a round part's split face as the feed takes it: blended from
/// the face's `corners` as the split says, at its own place and normal, and
/// where retail shaded the face, retail's shade at that normal (`normals`
/// are the corners' world normals as retail lit them).
fn round_vertex(
    corner: &crate::round_parts::SplitCorner,
    corners: &[SceneVertex],
    normals: &[Option<[i32; 3]>],
    factors: &[[u8; 3]],
    lighting: Option<crate::model_lighting::ModelLighting>,
) -> SceneVertex {
    use crate::round_parts::Blend;
    let weights: Corners<(usize, f32)> = match corner.blend {
        // A corner of the face's own keeps retail's vertex, unless a lug
        // sank it.
        Blend::Corner(index)
            if corner.position == corners[index].position
                && corner.normal == corners[index].normal =>
        {
            return corners[index]
        }
        Blend::Corner(index) => std::iter::once((index, 1.0)).collect(),
        Blend::Edge(from, to, t) => [(from, 1.0 - t), (to, t)].into_iter().collect(),
        Blend::Middle => {
            let share = 1.0 / corners.len() as f32;
            (0..corners.len()).map(|index| (index, share)).collect()
        }
        Blend::Mix(mix) => mix
            .into_iter()
            .filter(|&(_, weight)| weight > 0.0)
            .collect(),
    };
    let mix = |value: &dyn Fn(usize) -> f32| -> f32 {
        weights
            .iter()
            .map(|&(index, weight)| value(index) * weight)
            .sum()
    };
    let byte = |value: &dyn Fn(usize) -> u8| mix(&|index| f32::from(value(index))).round() as u8;
    let shaded = weights
        .iter()
        .all(|&(index, _)| corners[index].shade[3] == 255);
    // Retail's normals are 4096 long; a new corner's is scaled to its ends'.
    let lengths: Corners<f64> = weights
        .iter()
        .filter_map(|&(index, _)| normals.get(index).copied().flatten())
        .map(|normal| {
            normal
                .map(|axis| f64::from(axis) * f64::from(axis))
                .iter()
                .sum::<f64>()
                .sqrt()
        })
        .collect();
    let shade = match lighting.filter(|_| shaded && lengths.len() == weights.len()) {
        Some(lighting) => {
            let length = lengths.iter().sum::<f64>() / lengths.len() as f64;
            let world = corner
                .normal
                .map(|axis| (f64::from(axis) * length).round() as i32);
            let rgb = crate::model_lighting::shade_world(world, lighting);
            let [r, g, b] = brightened(
                rgb,
                std::array::from_fn(|k| byte(&|index| factors[index][k])),
            );
            [r, g, b, 255]
        }
        None if shaded => std::array::from_fn(|k| byte(&|index| corners[index].shade[k])),
        None => [0; 4],
    };
    SceneVertex {
        position: corner.position,
        normal: corner.normal,
        uv: [
            mix(&|index| corners[index].uv[0]),
            mix(&|index| corners[index].uv[1]),
        ],
        color: std::array::from_fn(|k| byte(&|index| corners[index].color[k])),
        shade,
        fog: std::array::from_fn(|k| byte(&|index| corners[index].fog[k])),
        motion: std::array::from_fn(|k| mix(&|index| corners[index].motion[k])),
    }
}

/// A blended face of a model the tuning makes glow (the crystals), gathered
/// into its light at the face's centre, in its sprite's colour.
fn glow_face<'a>(
    builder: &mut SceneFeedBuilder<'a>,
    request: &ModelPrimitiveRequest<'_>,
    face: &Face,
    id: i16,
    resource: crate::render_resources::RenderResource<'a>,
    mode: u32,
) {
    let Some(glow) = model_glow(request, id) else {
        return;
    };
    let corners: Vec<[f32; 3]> = face
        .slots
        .iter()
        .filter_map(|slot| request.vertices.drawn_view(slot.raw() as i16))
        .map(|view| builder.world(view))
        .collect();
    if corners.len() != face.slots.len() {
        return;
    }
    let centre = std::array::from_fn(|axis| {
        corners.iter().map(|corner| corner[axis]).sum::<f32>() / corners.len() as f32
    });
    add_glow(builder, request, glow, centre, id, resource, mode);
}

/// A glowing model's light at `centre`: in its sprite's colour, or the
/// tuning's own.
fn add_glow<'a>(
    builder: &mut SceneFeedBuilder<'a>,
    request: &ModelPrimitiveRequest<'_>,
    glow: crate::modern_tuning::Glow,
    centre: [f32; 3],
    id: i16,
    resource: crate::render_resources::RenderResource<'a>,
    mode: u32,
) {
    let (texture, color) = match glow.color {
        Some(rgb) => {
            let [r, g, b] = rgb.map(|channel| (channel * 255.0).round() as u8);
            (None, [r, g, b, 255])
        }
        None => (
            Some(builder.sprite(id, resource, sprite_mode(mode))),
            [255; 4],
        ),
    };
    builder.glow(
        centre,
        crate::scene_feed::SceneGlow {
            texture,
            color,
            strength: glow.strength,
            reach: glow.reach,
            gather: glow.gather.unwrap_or(0.0),
            model: request.model.identity.global_index,
            ..Default::default()
        },
    );
}

/// A blended edge of a model the tuning makes glow (the warpgate's
/// lightning), gathered into its light at the edge's middle if `additive`;
/// one of a model whose arcs span a field gives the field its shape.
fn glow_edge<'a>(
    builder: &mut SceneFeedBuilder<'a>,
    request: &ModelPrimitiveRequest<'_>,
    edge: &ModelEdge,
    id: i16,
    resource: crate::render_resources::RenderResource<'a>,
    mode: u32,
    additive: bool,
) {
    let glow = model_glow(request, id);
    let field = model_field(request);
    if glow.is_none() && field.is_none() {
        return;
    }
    let slots = edge.vertices.map(|slot| slot.raw() as i16);
    let (Some(start), Some(end)) = (
        request.vertices.drawn_view(slots[0]),
        request.vertices.drawn_view(slots[1]),
    ) else {
        return;
    };
    let [start, end] = [builder.world(start), builder.world(end)];
    if let Some((strength, color)) = field {
        builder.feed.arcs.push(crate::scene_feed::SceneArc {
            model: request.model.identity.global_index,
            ends: [start, end],
            strength,
            color,
        });
    }
    if let (Some(glow), true) = (glow, additive) {
        let centre = std::array::from_fn(|axis| 0.5 * (start[axis] + end[axis]));
        add_glow(builder, request, glow, centre, id, resource, mode);
    }
}

/// A blended edge of a gate the trace's frame redraws
/// ([`crate::scene_gate`]), kept as a bolt for it: fed, so the raster leaves
/// it while the world is traced.
fn gate_bolt<'a>(
    builder: &mut SceneFeedBuilder<'a>,
    request: &ModelPrimitiveRequest<'_>,
    edge: &ModelEdge,
    id: i16,
    resource: crate::render_resources::RenderResource<'a>,
    mode: u32,
) -> bool {
    let Some(look) = model_gate(request) else {
        return false;
    };
    let slots = edge.vertices.map(|slot| slot.raw() as i16);
    let (Some(start), Some(end)) = (
        request.vertices.drawn_view(slots[0]),
        request.vertices.drawn_view(slots[1]),
    ) else {
        return false;
    };
    let ends = [builder.world(start), builder.world(end)];
    let half_width = f32::from(edge.parameter as i16).abs();
    let sprite = builder.sprite(id, resource, sprite_mode(mode));
    if half_width > 0.0 {
        builder.feed.emitters.push(SceneEmitter {
            position: ends[0],
            span: std::array::from_fn(|axis| ends[1][axis] - ends[0][axis]),
            radius: half_width,
            texture: Some(sprite),
            color: [255; 3],
        });
    }
    builder.feed.bolts.push(SceneBolt {
        ends,
        half_width,
        sprite,
        look,
    });
    true
}

/// A flare (1B: its vertex slot, then its H3 sprite) of a gate the trace's
/// frame redraws, waiting for where its quad lands
/// ([`SceneFeedBuilder::place_gate_glint`]): fed, as its bolts are.
fn gate_flare<'a>(
    builder: &mut SceneFeedBuilder<'a>,
    resources: &RenderResources<'a>,
    request: &ModelPrimitiveRequest<'_>,
    arguments: &[u16],
    mode: u32,
) -> bool {
    let Some(look) = model_gate(request) else {
        return false;
    };
    let Some(&sprite) = arguments.get(1) else {
        return false;
    };
    let id = sprite as i16;
    if id < 0 || id as usize > resources.pool_capacity(RenderResourceKind::Sprite) {
        return false;
    }
    let Ok(resource) = resources.resolve(RenderResourceRequest {
        kind: RenderResourceKind::Sprite,
        argument: i32::from(id),
    }) else {
        return false;
    };
    let sprite = builder.sprite(id, resource, sprite_mode(mode));
    builder.gate_glint = Some((sprite, look));
    true
}

/// The look of the gate `request`'s model is, if the tuning redraws it.
fn model_gate(request: &ModelPrimitiveRequest<'_>) -> Option<usize> {
    let name = request.model.name.as_ref()?;
    let name = request
        .source
        .bytes
        .get(name.bytes.start..name.bytes.end)
        .unwrap_or_default();
    crate::modern_tuning::current()
        .materials()
        .gate_of(&String::from_utf8_lossy(name))
        .map(|(look, _)| look)
}

/// The field `request`'s model spans with its blended edges by the
/// tuning's fields, if any.
fn model_field(request: &ModelPrimitiveRequest<'_>) -> Option<(f32, [f32; 3])> {
    let name = request.model.name.as_ref()?;
    let name = request
        .source
        .bytes
        .get(name.bytes.start..name.bytes.end)
        .unwrap_or_default();
    crate::modern_tuning::current()
        .materials()
        .field_of(&String::from_utf8_lossy(name))
}

/// An additive billboard (1A, 1B: its vertex slot, then its H3 sprite) of a
/// model the tuning makes glow, gathered into its light at its anchor.
fn glow_billboard<'a>(
    builder: &mut SceneFeedBuilder<'a>,
    resources: &RenderResources<'a>,
    request: &ModelPrimitiveRequest<'_>,
    arguments: &[u16],
    mode: u32,
) {
    let (Some(&slot), Some(&sprite)) = (arguments.first(), arguments.get(1)) else {
        return;
    };
    let Some(glow) = model_glow(request, sprite as i16) else {
        return;
    };
    let Some(view) = request.vertices.drawn_view(slot as i16) else {
        return;
    };
    let id = sprite as i16;
    if id < 0 || id as usize > resources.pool_capacity(RenderResourceKind::Sprite) {
        return;
    }
    let Ok(resource) = resources.resolve(RenderResourceRequest {
        kind: RenderResourceKind::Sprite,
        argument: i32::from(id),
    }) else {
        return;
    };
    let RenderResourceData::Sprite(entry) = resource.data else {
        return;
    };
    let additive = entry
        .selectors
        .variant_index(sprite_mode(mode))
        .and_then(|selector| entry.variants.get(selector))
        .is_some_and(|variant| variant.flags.raw() & ADDITIVE_SPRITE_FLAG != 0);
    if !additive {
        return;
    }
    let centre = builder.world(view);
    add_glow(builder, request, glow, centre, id, resource, mode);
}

/// A billboard (1A, 1B) whose sprite `arguments[1]` adds and whose vertex
/// slot `arguments[0]` lies under the water's plane waits for its command's
/// corners as the tracer's glow ([`SceneFeedBuilder::sunk_billboard`]): the
/// raster's own draw would be hidden by the tracer's water (see
/// [`model_face`]'s `sunk`).
fn sink_billboard<'a>(
    builder: &mut SceneFeedBuilder<'a>,
    resources: &RenderResources<'a>,
    request: &ModelPrimitiveRequest<'_>,
    arguments: &[u16],
    mode: u32,
) {
    let Some(&sprite) = arguments.get(1) else {
        return;
    };
    let id = sprite as i16;
    if id < 0
        || id as usize > resources.pool_capacity(RenderResourceKind::Sprite)
        || !billboard_sunk(builder, request, arguments)
    {
        return;
    }
    let Ok(resource) = resources.resolve(RenderResourceRequest {
        kind: RenderResourceKind::Sprite,
        argument: i32::from(id),
    }) else {
        return;
    };
    let RenderResourceData::Sprite(entry) = resource.data else {
        return;
    };
    let additive = entry
        .selectors
        .variant_index(sprite_mode(mode))
        .and_then(|selector| entry.variants.get(selector))
        .is_some_and(|variant| variant.flags.raw() & ADDITIVE_SPRITE_FLAG != 0);
    if additive {
        let index = builder.sprite(id, resource, sprite_mode(mode));
        builder.sunk_billboard = Some((index, request.model.identity.global_index));
    }
}

/// Whether the billboard at `arguments[0]`'s vertex slot lies under the
/// water's plane, seen from above it ([`sunk_below`]).
fn billboard_sunk(
    builder: &SceneFeedBuilder<'_>,
    request: &ModelPrimitiveRequest<'_>,
    arguments: &[u16],
) -> bool {
    let Some(&slot) = arguments.first() else {
        return false;
    };
    let Some(view) = request.vertices.drawn_view(slot as i16) else {
        return false;
    };
    let point = builder.world(view);
    sunk_below(builder.water_plane, std::slice::from_ref(&point))
}

/// The light `request`'s model gives from its parts in sprite `sprite`
/// by the tuning's glows, if any.
fn model_glow(
    request: &ModelPrimitiveRequest<'_>,
    sprite: i16,
) -> Option<crate::modern_tuning::Glow> {
    let name = model_name(request)?;
    crate::modern_tuning::current()
        .materials()
        .glow_of(&String::from_utf8_lossy(name), sprite)
}

/// How `request`'s model's faces in sprite `sprite` glow with its vehicle's
/// thrust, if the tuning says they do (the skimmer's lights).
fn thrust_glow(
    builder: &mut SceneFeedBuilder<'_>,
    request: &ModelPrimitiveRequest<'_>,
    sprite: i16,
) -> Option<crate::modern_tuning::ThrustGlow> {
    let tuning = crate::modern_tuning::current();
    let name = model_name(request)?;
    // Most models have none: each model's answer is kept for the frame.
    let has = *builder
        .thrust_models
        .entry(request.model.identity.global_index)
        .or_insert_with(|| {
            tuning
                .materials()
                .glows_with_thrust(&String::from_utf8_lossy(name))
        });
    if !has {
        return None;
    }
    tuning
        .materials()
        .thrust_glow_of(&String::from_utf8_lossy(name), sprite)
        .cloned()
}

/// The exhausts the tuning's trails name on `request`'s model, as drawn: each group of slots' middle, pushing away
/// from its `from` group.
fn trail_exhausts(builder: &mut SceneFeedBuilder<'_>, request: &ModelPrimitiveRequest<'_>) {
    let Some(name) = model_name(request) else {
        return;
    };
    let tuning = crate::modern_tuning::current();
    // Most models have none: each model's answer is kept for the frame.
    let has = *builder
        .exhaust_models
        .entry(request.model.identity.global_index)
        .or_insert_with(|| {
            let name = String::from_utf8_lossy(name);
            let has = tuning.trails().exhausts_of(&name).next().is_some();
            has
        });
    if !has {
        return;
    }
    let name = String::from_utf8_lossy(name);
    // The middle of a group of slots as drawn, the model's origin for none.
    let middle = |builder: &SceneFeedBuilder<'_>, group: &[i16]| -> Option<[f32; 3]> {
        if group.is_empty() {
            return Some(builder.world(request.transform.view_position));
        }
        let mut sum = [0.0_f32; 3];
        for &slot in group {
            let at = builder.world(request.vertices.drawn_view(slot)?);
            for axis in 0..3 {
                sum[axis] += at[axis] / group.len() as f32;
            }
        }
        Some(sum)
    };
    for exhaust in tuning.trails().exhausts_of(&name) {
        for (index, group) in exhaust.points.iter().enumerate() {
            let Some(position) = middle(builder, group) else {
                continue;
            };
            let away = exhaust
                .from
                .get(index)
                .and_then(|from| middle(builder, from))
                .map(|from| std::array::from_fn(|axis| position[axis] - from[axis]))
                .unwrap_or([0.0; 3]);
            let length = away.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
            let push = if length > 0.0 {
                away.map(|axis| axis / length * exhaust.push)
            } else {
                [0.0; 3]
            };
            builder.exhaust(crate::scene_feed::SceneExhaust {
                position,
                push,
                look: exhaust.look,
            });
        }
    }
}

/// The name of `request`'s model, if it has one.
pub(crate) fn model_name<'a>(request: &ModelPrimitiveRequest<'a>) -> Option<&'a [u8]> {
    let name = request.model.name.as_ref()?;
    Some(
        request
            .source
            .bytes
            .get(name.bytes.start..name.bytes.end)
            .unwrap_or_default(),
    )
}

/// Retail's one folded quad (every model's quads read, 2026-10-24): the grey
/// underside of the hovercraft's duct (`scatfan`), slots [20, 21, 29, 21]
/// lit from [44, 45, 45, 52], repeats its second corner for its fourth. It
/// covers half of the black face drawn over the same corners and slot 28
/// ([28, 20, 21, 29]), so the tracer's rays took the two in turns there, and
/// a round duct's black face bulges past it, leaving it standing inside the
/// duct. The tracer gives it slot 28 for that corner, lit from 44 as the
/// face beside it lights slot 28, so it pairs with the black face as each
/// grey face of the duct pairs with its own. The raster draws retail's.
fn unfold_duct(name: Option<&[u8]>, slots: &mut [ModelSlotRef], lighting: &mut [ModelSlotRef]) {
    let raw = |refs: &[ModelSlotRef]| refs.iter().map(|slot| slot.raw()).collect::<Vec<_>>();
    if name == Some(&b"scatfan"[..])
        && raw(slots) == [20, 21, 29, 21]
        && raw(lighting) == [44, 45, 45, 52]
    {
        slots[3] = ModelSlotRef::from_raw(28);
        lighting[3] = ModelSlotRef::from_raw(44);
    }
}

/// Retail's blast wave (`exwave*`, the fading visual a blast builds: its
/// blue streaks drawn as edges) where it stands under the water's plane: the
/// traced water hides it, as it hides the blast's fireballs, and the
/// eruption replaces it (2026-10-23, on the streaks). Under the plane and not in water is under the ground,
/// hidden either way.
fn blast_wave_under_water(name: Option<&[u8]>, height: f32, plane: f32) -> bool {
    name.is_some_and(|name| name.starts_with(b"exwave")) && height < plane
}

/// The ACAM's wheel models, each drawn by its own axle with its origin at
/// the hub (`docs/rt-plans/round-models.md`): the buggy's `huskywheel` and
/// `huskywheelsimple` (radius 32), the truck's `bigballwheel` and
/// `bigballwheelsimple` (radius 50).
const WHEEL_MODELS: [&[u8]; 2] = [b"huskywheel", b"bigballwheel"];

fn is_wheel(name: Option<&[u8]>) -> bool {
    name.is_some_and(|name| WHEEL_MODELS.iter().any(|wheel| name.starts_with(wheel)))
}

/// Remember the name of `request`'s model, which its tuning goes by.
fn name_model(builder: &mut SceneFeedBuilder<'_>, request: &ModelPrimitiveRequest<'_>) {
    let model = request.model.identity.global_index;
    if builder.feed.model_names.contains_key(&model) {
        return;
    }
    if let Some(name) = request.model.name.as_ref() {
        let bytes = request
            .source
            .bytes
            .get(name.bytes.start..name.bytes.end)
            .unwrap_or_default();
        builder
            .feed
            .model_names
            .insert(model, String::from_utf8_lossy(bytes).into());
    }
}

/// An edge (opcode 07) for the tracer. Retail widens it on screen into a
/// strip of its sprite, `parameter` world units to each side at its mean
/// depth, the sprite's width along the edge and its ends past the points
/// by a cap (`model_primitives/edge.rs`). Here the same strip stands in
/// the world facing the eye, its masked texels open, so the tracer lights
/// it as a rod and it casts shadows: the markers' legs, the axles. A
/// blended sprite stays the raster's, as a face's does.
fn capture_edge<'a>(
    builder: &mut SceneFeedBuilder<'a>,
    resources: &RenderResources<'a>,
    request: &ModelPrimitiveRequest<'_>,
    edge: &ModelEdge,
    mode: u32,
    far_color: u32,
) -> bool {
    let model = request.model.identity.global_index;
    let id = edge.edge_type as i16;
    if id < 0 || id as usize > resources.pool_capacity(RenderResourceKind::Sprite) {
        return false;
    }
    let Ok(resource) = resources.resolve(RenderResourceRequest {
        kind: RenderResourceKind::Sprite,
        argument: i32::from(id),
    }) else {
        return false;
    };
    let RenderResourceData::Sprite(entry) = resource.data else {
        return false;
    };
    let Some(selector) = entry.selectors.variant_index(sprite_mode(mode)) else {
        return true;
    };
    let (Some(variant), Some(shape)) = (entry.variants.get(selector), entry.variants.first())
    else {
        return false;
    };
    // Blended edges stay on the raster; a glowing model's additive ones
    // (the warpgate's lightning) still light the world.
    if variant.flags.raw() & TRANSLUCENT_SPRITE_FLAGS != 0 {
        if gate_bolt(builder, request, edge, id, resource, mode) {
            return true;
        }
        let additive = variant.flags.raw() & ADDITIVE_SPRITE_FLAG != 0;
        glow_edge(builder, request, edge, id, resource, mode, additive);
        return false;
    }
    let slots = edge.vertices.map(|slot| slot.raw() as i16);
    let (Some(start), Some(end)) = (
        request.vertices.drawn_view(slots[0]),
        request.vertices.drawn_view(slots[1]),
    ) else {
        return false;
    };
    let points = [builder.world(start), builder.world(end)];
    let motions = [0, 1].map(|end| {
        builder.vertex_motion(
            request.model.identity.global_index,
            request.transform,
            slots[end] as u16,
            points[end],
        )
    });
    // The sprite's ends cap the edge: its width runs along it, the middle
    // (width - height) of it between the points.
    let cap = if shape.width > shape.height {
        f32::from(shape.height) / (2.0 * f32::from(shape.width - shape.height))
    } else {
        0.0
    };
    let ends = [
        std::array::from_fn(|i| points[0][i] - (points[1][i] - points[0][i]) * cap),
        std::array::from_fn(|i| points[1][i] + (points[1][i] - points[0][i]) * cap),
    ];
    let far = (request.dispatch == ModelDispatch::Far)
        .then(|| crate::d3d_effect::far_rgb(far_color, builder.material_shifts));
    let corner = |slot: i16, u: f32, motion: [f32; 3]| SceneVertex {
        uv: [u, 0.0],
        color: [255; 4],
        motion,
        fog: far
            .and_then(|[r, g, b]| {
                crate::model_primitives::vertex_fog::<()>(request, usize::from(slot as u16))
                    .ok()
                    .map(|amount| [r, g, b, amount])
            })
            .unwrap_or([0; 4]),
        ..SceneVertex::default()
    };
    let light = match request.shades.lighting() {
        Some(lighting) if !builder.lit_by_trace => {
            Some(builder.model_light(SceneModelLight::from_model(lighting)))
        }
        _ => None,
    };
    let sprite = builder.sprite(id, resource, sprite_mode(mode));
    name_model(builder, request);
    builder.ribbon(
        ends,
        f32::from(edge.parameter as i16).abs(),
        [
            corner(slots[0], 0.0, motions[0]),
            corner(slots[1], 1.0, motions[1]),
        ],
        Some(sprite),
        Some(model),
        light,
        variant.flags.raw() & 1 != 0,
    );
    true
}

/// A blended quad the raster draws (a particle's sprite, a bolt's streak,
/// a beam segment, a model's effect billboard or edge), kept as an emitter
/// where its sprite adds light, for mirrored effects: a disc at the quad's
/// middle or a segment along its span, from the native corners the command
/// draws it at, out to the fog's end (past it, only the sky's art). One the
/// tracer already took in 3D or hides under the water is skipped.
pub(crate) fn capture_emitter<'a>(
    builder: &mut SceneFeedBuilder<'a>,
    command: &PrimitiveCommand<'a>,
) {
    if command.scene_fed || command.glow_fed {
        return;
    }
    let (vertices, resource, mode, flags, color, allow_segment) = match command.callback {
        PrimitiveCallback::TexturedQuadNear | PrimitiveCallback::TexturedQuadFar => {
            let Ok(Some(quad)) = crate::d3d_textured::read_textured_quad(command) else {
                return;
            };
            let mode = sprite_mode(quad.mode);
            (
                quad.vertices,
                quad.resource,
                mode,
                quad.flags,
                [255; 3],
                false,
            )
        }
        PrimitiveCallback::EffectQuadNear | PrimitiveCallback::EffectQuadFar => {
            let Ok(Some(quad)) = crate::d3d_effect::read_effect_quad(command) else {
                return;
            };
            let mode = quad.sprite_mode;
            let peak = quad.effect_rgb.into_iter().max().unwrap_or(0);
            let color = if peak > 0 {
                quad.effect_rgb.map(|channel| {
                    ((u32::from(channel) * 255 + u32::from(peak) / 2) / u32::from(peak)) as u8
                })
            } else {
                [255; 3]
            };
            (quad.vertices, quad.resource, mode, quad.flags, color, true)
        }
        PrimitiveCallback::D6QuadNear | PrimitiveCallback::D6QuadFar => {
            let Ok(Some(quad)) = crate::d3d_d6::read_d6(command) else {
                return;
            };
            let mode = sprite_mode(quad.mode);
            let mean: [u8; 3] = std::array::from_fn(|channel| {
                (quad
                    .shades
                    .iter()
                    .map(|shade| u32::from(shade[channel]))
                    .sum::<u32>()
                    / 4) as u8
            });
            let peak = mean.into_iter().max().unwrap_or(0);
            let color = if peak > 0 {
                mean.map(|channel| {
                    ((u32::from(channel) * 255 + u32::from(peak) / 2) / u32::from(peak)) as u8
                })
            } else {
                [255; 3]
            };
            (quad.vertices, quad.resource, mode, quad.flags, color, true)
        }
        _ => return,
    };
    if flags & ADDITIVE_SPRITE_FLAG == 0 {
        return;
    }
    let corners = vertices.map(|vertex| [vertex.x, vertex.y, vertex.depth as f32]);
    let Some((position, span, radius)) =
        quad_emitter_shape(corners, &builder.feed.camera, allow_segment)
    else {
        return;
    };
    let reach = builder.feed.fog.end;
    let mid: [f32; 3] = std::array::from_fn(|axis| position[axis] + 0.5 * span[axis]);
    if mid.iter().map(|axis| axis * axis).sum::<f32>() > reach * reach {
        return;
    }
    let texture = builder.sprite(resource.request.argument as i16, resource, mode);
    builder.feed.emitters.push(SceneEmitter {
        position,
        span,
        radius,
        texture: Some(texture),
        color,
    });
}

fn quad_emitter_shape(
    corners: [[f32; 3]; 4],
    camera: &SceneCamera,
    allow_segment: bool,
) -> Option<([f32; 3], [f32; 3], f32)> {
    if allow_segment {
        if corners.iter().any(|corner| corner[2] < camera.near) {
            return None;
        }
        let [right, up, forward] = camera.basis;
        let world = corners.map(|[x, y, depth]| {
            let view = [
                (x - camera.centre[0]) * depth / camera.focal[0],
                (camera.centre[1] - y) * depth / camera.focal[1],
                depth,
            ];
            std::array::from_fn(|axis| {
                right[axis] * view[0] + up[axis] * view[1] + forward[axis] * view[2]
            })
        });
        let length = |a: [f32; 3], b: [f32; 3]| {
            (0..3)
                .map(|axis| (a[axis] - b[axis]).powi(2))
                .sum::<f32>()
                .sqrt()
        };
        let midpoint = |a: [f32; 3], b: [f32; 3]| -> [f32; 3] {
            std::array::from_fn(|axis| 0.5 * (a[axis] + b[axis]))
        };
        let mid03 = midpoint(world[0], world[3]);
        let mid12 = midpoint(world[1], world[2]);
        let len03_12 = length(mid03, mid12);
        let width03_12 = 0.5 * (length(world[0], world[3]) + length(world[1], world[2]));
        let mid01 = midpoint(world[0], world[1]);
        let mid23 = midpoint(world[2], world[3]);
        let len01_23 = length(mid01, mid23);
        let width01_23 = 0.5 * (length(world[0], world[1]) + length(world[2], world[3]));
        let depth_varies = corners.windows(2).any(|w| (w[0][2] - w[1][2]).abs() > 0.5);
        if (depth_varies || len03_12 > 1.5 * width03_12) && len03_12 >= len01_23 && width03_12 > 0.0
        {
            let span = std::array::from_fn(|axis| mid12[axis] - mid03[axis]);
            return Some((mid03, span, 0.5 * width03_12));
        }
        if (depth_varies || len01_23 > 1.5 * width01_23) && width01_23 > 0.0 {
            let span = std::array::from_fn(|axis| mid23[axis] - mid01[axis]);
            return Some((mid01, span, 0.5 * width01_23));
        }
    }
    let (position, radius) = quad_emitter(corners, camera)?;
    Some((position, [0.0; 3], radius))
}

/// A quad drawn at native `corners` (x, y and view depth) as `camera` sees
/// it, as a disc: its middle, eye-relative, and half the geometric mean of
/// its sides, so a streak keeps its own width more than its length. None at
/// or before the near plane.
fn quad_emitter(corners: [[f32; 3]; 4], camera: &SceneCamera) -> Option<([f32; 3], f32)> {
    if corners.iter().any(|corner| corner[2] < camera.near) {
        return None;
    }
    let [right, up, forward] = camera.basis;
    let world = corners.map(|[x, y, depth]| {
        let view = [
            (x - camera.centre[0]) * depth / camera.focal[0],
            (camera.centre[1] - y) * depth / camera.focal[1],
            depth,
        ];
        std::array::from_fn(|axis| {
            right[axis] * view[0] + up[axis] * view[1] + forward[axis] * view[2]
        })
    });
    let middle =
        std::array::from_fn(|axis| world.iter().map(|corner| corner[axis]).sum::<f32>() / 4.0);
    let length = |a: [f32; 3], b: [f32; 3]| {
        (0..3)
            .map(|axis| (a[axis] - b[axis]).powi(2))
            .sum::<f32>()
            .sqrt()
    };
    let radius = 0.5 * (length(world[0], world[1]) * length(world[1], world[2])).sqrt();
    (radius > 0.0).then_some((middle, radius))
}

#[cfg(test)]
mod tests {
    use super::{
        blast_wave_under_water, brightened, quad_emitter, quad_emitter_shape, sunk_below, tinted,
        unfold_duct,
    };
    use inf_formats::{ModelInstructionKind, ModelQuadLighting, ModelSlotRef};

    /// An additive sprite sunk under the water's plane is the tracer's
    /// (its glow), one at or above the surface stays the raster's blend, as
    /// does every one while the eye is under the water, and a world without
    /// water never sinks anything.
    #[test]
    fn only_a_sprite_below_the_water_goes_to_the_tracer() {
        let plane = Some(-200.0);
        // A token on the bottom, 150 below the surface.
        assert!(sunk_below(plane, &[[0.0, -350.0, 0.0]; 4]));
        // At the surface, and a unit under it: still the raster's blend,
        // which the water does not hide there.
        assert!(!sunk_below(plane, &[[0.0, -200.0, 0.0]; 4]));
        assert!(!sunk_below(plane, &[[0.0, -200.5, 0.0]; 4]));
        // The middle decides a face that straddles the surface.
        assert!(sunk_below(
            plane,
            &[
                [0.0, -100.0, 0.0],
                [0.0, -400.0, 0.0],
                [0.0, -400.0, 0.0],
                [0.0, -400.0, 0.0]
            ]
        ));
        assert!(!sunk_below(
            plane,
            &[
                [0.0, 100.0, 0.0],
                [0.0, -100.0, 0.0],
                [0.0, -100.0, 0.0],
                [0.0, -100.0, 0.0]
            ]
        ));
        // Dry worlds, and faces with no corners, change nothing.
        assert!(!sunk_below(None, &[[0.0, -9000.0, 0.0]; 4]));
        assert!(!sunk_below(plane, &[]));
        // Above the water it never fires, however high the ground is.
        assert!(!sunk_below(plane, &[[0.0, 500.0, 0.0]; 4]));
        // With the eye under the water (its plane above the eye), the
        // raster's blend shows over the traced bed: nothing sinks.
        assert!(!sunk_below(Some(50.0), &[[0.0, -350.0, 0.0]; 4]));
    }

    /// A fan corner's factors give the raster's colour times shade times
    /// them: the red cross's red keeps red and loses green and blue, its
    /// white doubles the shade, capped as the raster caps it.
    #[test]
    fn fan_factors_colour_the_corner_as_the_raster_does() {
        let grey = [200, 200, 200];
        assert_eq!(tinted(grey, [128, 0, 0]), [200, 0, 0]);
        assert_eq!(brightened([100, 100, 100], [128, 0, 0]), [100, 100, 100]);
        assert_eq!(tinted(grey, [255, 255, 255]), grey);
        assert_eq!(
            brightened([100, 100, 200], [255, 255, 255]),
            [199, 199, 255]
        );
        assert_eq!(tinted(grey, [128; 3]), grey);
        assert_eq!(brightened([100, 100, 100], [128; 3]), [100, 100, 100]);
    }

    /// A sprite drawn 64 pixels wide at depth 640 by a focal of 320 is 128
    /// units wide: a disc of that width at its middle, there along the
    /// view; a streak four times as long as it is wide is a disc twice its
    /// width across; one before the near plane is none.
    #[test]
    fn a_drawn_quad_becomes_a_disc_where_it_lies() {
        let camera = crate::scene_feed::SceneCamera {
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            focal: [320.0, 320.0],
            centre: [320.0, 240.0],
            near: 64.0,
            ..Default::default()
        };
        let quad = |left: f32, top: f32, right: f32, bottom: f32, depth: f32| {
            [
                [left, top, depth],
                [right, top, depth],
                [right, bottom, depth],
                [left, bottom, depth],
            ]
        };
        let (middle, radius) = quad_emitter(quad(288.0, 208.0, 352.0, 272.0, 640.0), &camera)
            .expect("in front of the eye");
        assert_eq!((middle, radius), ([0.0, 0.0, 640.0], 64.0));
        let (_, streak) = quad_emitter(quad(288.0, 232.0, 352.0, 248.0, 640.0), &camera)
            .expect("in front of the eye");
        assert_eq!(streak, 32.0);
        assert!(quad_emitter(quad(288.0, 208.0, 352.0, 272.0, 32.0), &camera).is_none());
    }

    /// A beam's quad (a laser, barrier or ribbon segment: its two ends at
    /// their own depths, widened across the screen) becomes a segment from
    /// one end's middle to the other's, half its width across; a square
    /// billboard stays a disc, and a particle's sprite is always one.
    #[test]
    fn a_beam_quad_becomes_a_segment_and_a_billboard_a_disc() {
        let camera = crate::scene_feed::SceneCamera {
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            focal: [320.0, 320.0],
            centre: [320.0, 240.0],
            near: 64.0,
            ..Default::default()
        };
        // From (0, 0, 640) to (0, 0, 1280), 6 pixels wide at each end: the
        // native order previous - offset, current - offset, current +
        // offset, previous + offset.
        let beam = [
            [317.0, 240.0, 640.0],
            [317.0, 240.0, 1280.0],
            [323.0, 240.0, 1280.0],
            [323.0, 240.0, 640.0],
        ];
        let (start, span, radius) = quad_emitter_shape(beam, &camera, true).expect("in view");
        assert!(start
            .iter()
            .zip([0.0, 0.0, 640.0])
            .all(|(a, b)| (a - b).abs() < 1e-3));
        assert!(span
            .iter()
            .zip([0.0, 0.0, 640.0])
            .all(|(a, b)| (a - b).abs() < 1e-3));
        // Six pixels at 640 and 1280 deep by a focal of 320: 12 and 24 units.
        assert!((radius - 9.0).abs() < 1e-3);
        let square = [
            [288.0, 208.0, 640.0],
            [352.0, 208.0, 640.0],
            [352.0, 272.0, 640.0],
            [288.0, 272.0, 640.0],
        ];
        let (middle, span, radius) = quad_emitter_shape(square, &camera, true).unwrap();
        assert_eq!((middle, span, radius), ([0.0, 0.0, 640.0], [0.0; 3], 64.0));
        let (_, span, _) = quad_emitter_shape(beam, &camera, false).unwrap();
        assert_eq!(span, [0.0; 3], "a particle's sprite is a disc");
    }

    /// The blast wave's models hide under the water's plane, not above it;
    /// other models and unnamed ones never.
    #[test]
    fn the_blast_wave_hides_under_the_water() {
        assert!(blast_wave_under_water(Some(b"exwave8"), -300.0, -200.0));
        assert!(!blast_wave_under_water(Some(b"exwave8"), -100.0, -200.0));
        assert!(!blast_wave_under_water(Some(b"gatebod"), -300.0, -200.0));
        assert!(!blast_wave_under_water(None, -300.0, -200.0));
    }

    /// The duct's folded underside is the only quad of a world's models
    /// that repeats a corner out of turn; unfolded, it stands on the black
    /// face's corners, as its siblings stand on theirs, lit as theirs are.
    #[test]
    fn the_ducts_folded_underside_unfolds_onto_its_black_face() {
        fn quads(program: &inf_formats::ModelProgram, found: &mut Vec<(Vec<u16>, Vec<u16>)>) {
            for instruction in &program.instructions {
                match &instruction.kind {
                    ModelInstructionKind::Quad(quad) => found.push((
                        quad.vertices.iter().map(|slot| slot.raw()).collect(),
                        match &quad.lighting {
                            ModelQuadLighting::Shared(slot) => vec![slot.raw(); 4],
                            ModelQuadLighting::PerVertex(slots) => {
                                slots.iter().map(|slot| slot.raw()).collect()
                            }
                        },
                    )),
                    ModelInstructionKind::NestedProgram(nested) => quads(&nested.program, found),
                    _ => {}
                }
            }
        }
        let folded = |slots: &[u16]| (0..2).any(|first| slots[first] == slots[first + 2]);
        let (residency, _, _) = crate::test_worlds::offline_world_retained(
            18,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let mut duct = None;
        for slot in 0..4096 {
            let Ok(Some(binding)) =
                crate::object_runtime::resident_model_resource(&residency, slot)
            else {
                continue;
            };
            let name = binding
                .model
                .name
                .as_ref()
                .and_then(|name| name.ascii(binding.allocation.bytes()))
                .unwrap_or_default();
            let mut found = Vec::new();
            quads(&binding.model.program, &mut found);
            if found.iter().any(|(slots, _)| folded(slots)) {
                assert_eq!(name, "scatfan", "model {slot}");
                duct = Some(found);
            }
        }
        let faces = duct.expect("the duct's model is resident");
        let to_refs = |raw: &[u16]| raw.iter().copied().map(ModelSlotRef::from_raw).collect();
        let (mut slots, mut lighting): (Vec<ModelSlotRef>, Vec<ModelSlotRef>) = faces
            .iter()
            .find(|(slots, _)| folded(slots))
            .map(|(slots, lighting)| (to_refs(slots), to_refs(lighting)))
            .unwrap();
        unfold_duct(Some(b"scatfan"), &mut slots, &mut lighting);
        let raw = |refs: &[ModelSlotRef]| refs.iter().map(|slot| slot.raw()).collect::<Vec<_>>();
        let [slots, lighting] = [raw(&slots), raw(&lighting)];
        assert!(!folded(&slots));
        // The black face over the same corners, a turn round from it, as each
        // grey sibling's black face is from it.
        let turned = |slots: &[u16]| vec![slots[3], slots[0], slots[1], slots[2]];
        let paired = |slots: &[u16]| faces.iter().any(|(black, _)| *black == turned(slots));
        assert!(paired(&slots));
        assert_eq!(
            faces
                .iter()
                .filter(|(grey, light)| light[0] != light[1] && paired(grey))
                .count(),
            3
        );
        // The face beside it lights slot 28 from 44 too.
        assert!(faces.iter().any(|(sibling, light)| sibling != &slots
            && (0..4).any(|corner| sibling[corner] == 28 && light[corner] == 44)));
        assert_eq!(lighting[3], 44);
        // Another model's quad stays as it is.
        let mut other = to_refs(&[20, 21, 29, 21]);
        let mut other_light = to_refs(&[44, 45, 45, 52]);
        unfold_duct(Some(b"hoverjet2"), &mut other, &mut other_light);
        assert_eq!(raw(&other), [20, 21, 29, 21]);
    }
}
