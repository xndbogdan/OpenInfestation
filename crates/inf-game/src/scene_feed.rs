//! The world frame's scene in world space, for the Vulkan device's path
//! tracer ([the plan](../../../docs/INFESTATION-RT-IMPLEMENTATION.md)).
//!
//! Read-only taps beside the game's own draws copy what those draws already
//! produced: model and terrain triangles before projection, the camera, the
//! sun and the point lights. Nothing here feeds back into the game, so
//! gameplay, replays and every renderer's frames stay as they were.
//!
//! Positions are relative to the eye, in world units, so the 16-bit world's
//! wrap never reaches the tracer and `f32` keeps its precision near the eye.

use std::rc::Rc;

use inf_formats::SpriteRenderMode;

use crate::{
    model_lighting::ModelLighting, render_resources::RenderResource, scene_terrain::TerrainMesh,
};

/// One vertex of the feed: eye-relative position, the world normal it is
/// lit along (zero where none is known: the face's own then), texture
/// coordinates over the sprite (0..1), its colour, the shade
/// retail lit it with (a shade of 128 is one; alpha 0 where it has none),
/// and the fog retail gave it (the far colour, and in alpha how much).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [u8; 4],
    pub shade: [u8; 4],
    pub fog: [u8; 4],
    /// Where this point was in the last traced frame, less where it is now,
    /// in world units: a moving model's own motion, which the upscalers and
    /// the anti-aliasing follow. Zero for what keeps still.
    pub motion: [f32; 3],
}

/// Where each drawn object's model vertices were in the last traced frame,
/// as absolute world positions, so a moving model's pixels report its own
/// motion rather than the camera's. The frame keeps it between feeds.
#[derive(Debug, Default)]
pub struct MotionHistory {
    last: MotionFrame,
    now: MotionFrame,
    /// The last key looked up, with its places in `now` and `last`: a
    /// draw's vertices come one after another under one key.
    cursor: Option<(MotionKey, usize, Option<usize>)>,
}

/// One frame's positions: each key's slot list, found through `index`.
#[derive(Debug, Default)]
struct MotionFrame {
    index: crate::fast_hash::FastMap<MotionKey, usize>,
    slots: Vec<Vec<Option<[f64; 3]>>>,
}

impl MotionFrame {
    fn clear(&mut self) {
        self.index.clear();
        self.slots.clear();
    }
}

/// A model an object drew: the object, the model, and how many draws of
/// that model the object had made before it in the frame (the wheels).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MotionKey {
    object: crate::object_runtime::ObjectId,
    model: u16,
    ordinal: u16,
}

/// The models the object drawing now has drawn so far this frame, each by
/// its model and transform with its ordinal among the object's draws of
/// that model: the key its motion is remembered by.
#[derive(Debug, Default)]
pub(crate) struct ObjectDraws {
    object: Option<crate::object_runtime::ObjectId>,
    draws: Vec<(u16, crate::model_context::ModelTransform, u16)>,
}

impl ObjectDraws {
    /// `object`'s models start drawing.
    pub(crate) fn begin(&mut self, object: crate::object_runtime::ObjectId) {
        self.object = Some(object);
        self.draws.clear();
    }

    /// The object's models are drawn.
    pub(crate) fn end(&mut self) {
        self.object = None;
    }

    /// The key of model `model` drawn in `transform` by the object drawing
    /// now: none outside an object's draw.
    pub(crate) fn key(
        &mut self,
        model: u16,
        transform: crate::model_context::ModelTransform,
    ) -> Option<MotionKey> {
        let object = self.object?;
        let ordinal = match self
            .draws
            .iter()
            .find(|&&(drawn, at, _)| drawn == model && at == transform)
        {
            Some(&(_, _, ordinal)) => ordinal,
            None => {
                let ordinal = self
                    .draws
                    .iter()
                    .filter(|&&(drawn, _, _)| drawn == model)
                    .count() as u16;
                self.draws.push((model, transform, ordinal));
                ordinal
            }
        };
        Some(MotionKey {
            object,
            model,
            ordinal,
        })
    }
}

/// The most a point moves between two frames and still counts as motion:
/// past it, a respawn or a jump, which starts again.
const MOST_MOTION: f64 = 2048.0;

impl MotionHistory {
    /// The frame is traced: its positions become the last frame's.
    pub fn finish_frame(&mut self) {
        std::mem::swap(&mut self.last, &mut self.now);
        self.now.clear();
        self.cursor = None;
    }

    /// Record vertex `slot` of `key` at `at` and give where it was in the
    /// last frame, less `at`, across the world's wrap in X and Z: zero on
    /// its first frame.
    fn displacement(&mut self, key: MotionKey, slot: usize, at: [f64; 3]) -> [f32; 3] {
        let (now_at, last_at) = match self.cursor {
            Some((known, now_at, last_at)) if known == key => (now_at, last_at),
            _ => {
                let now_at = *self.now.index.entry(key).or_insert_with(|| {
                    self.now.slots.push(Vec::new());
                    self.now.slots.len() - 1
                });
                let last_at = self.last.index.get(&key).copied();
                self.cursor = Some((key, now_at, last_at));
                (now_at, last_at)
            }
        };
        let now = &mut self.now.slots[now_at];
        if now.len() <= slot {
            now.resize(slot + 1, None);
        }
        now[slot] = Some(at);
        let Some(Some(then)) = last_at.and_then(|last| self.last.slots[last].get(slot)) else {
            return [0.0; 3];
        };
        let delta: [f64; 3] = std::array::from_fn(|axis| {
            let delta = then[axis] - at[axis];
            if axis == 1 {
                delta
            } else {
                (delta + 32768.0).rem_euclid(65536.0) - 32768.0
            }
        });
        if delta.iter().any(|delta| delta.abs() > MOST_MOTION) {
            return [0.0; 3];
        }
        delta.map(|delta| delta as f32)
    }
}

/// How a triangle's surface takes light in the tracer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneSurface {
    /// Lit by the tracer from its texture and vertex colour.
    #[default]
    Opaque,
    /// Drawn unlit by retail (opcodes D8, DA, E0 and E2): it keeps its
    /// brightness and lights what is near it.
    Emissive,
    /// Drawn unlit as [`Self::Emissive`] is, but seen rather than a light
    /// the tracer samples: the modern sky's land band and the lamps' lenses
    /// (whose lamps light as lamps).
    Shown,
    /// The world's liquid surface.
    Water,
    /// Retail's painted backdrop, the ring the sky shows on: the raster
    /// still draws it, and the tracer's reflections see it.
    Sky,
    /// An additive sprite sunk under the water (a token on a lake's bed):
    /// the raster's blend would fail the traced water's depth, so the
    /// water adds its texels where the view through it crosses them, as
    /// retail's blend adds them (pt_trace.comp's sunk_glow). It stops no
    /// ray and casts no shadow.
    Glow,
}

/// A triangle of three feed vertices with its texture (an index into
/// [`SceneFeed::textures`], or none) and surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneTriangle {
    pub vertices: [u32; 3],
    pub texture: Option<u32>,
    pub surface: SceneSurface,
    /// The model the triangle came from (an H8 slot), or none for terrain.
    pub model: Option<u16>,
    /// The light its model was shaded with: an index into
    /// [`SceneFeed::model_lights`], or none for the world's sun.
    pub light: Option<u32>,
    /// Its sprite is masked: the raster's blend shows through its keyed
    /// texels (alpha 0), which rays pass.
    pub cutout: bool,
    /// Its draw fogs it over a range or in a colour of its own (retail's
    /// actor flash, `0047DEA0`: a hit vehicle's red): the distance fog
    /// leaves it its vertex fog.
    pub own_fog: bool,
    /// The object whose draw made it: an index into
    /// [`SceneFeed::objects`], or none outside an object's draw.
    pub object: Option<u32>,
    /// Its object lies outside the view (or is the craft round a
    /// first-person eye), drawn for the tracer alone
    /// ([`SceneFeedBuilder::culled`]): the scene's own rays meet it
    /// (shadows, reflections, bounces), the eye's do not.
    pub culled: bool,
}

/// An object drawn this frame, for thermal vision's heat by object: which
/// it is, where (world units, wrapping at 16 bits), and its root model (an
/// H8 slot, as [`SceneTriangle::model`]; none when it has none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneObject {
    pub id: crate::object_runtime::ObjectId,
    pub position: [i32; 3],
    pub root: Option<u16>,
}

/// The light retail shaded a model with (`004901B0`'s inputs), which the
/// draws set per model: the world's tint plus the light grid at the model,
/// or a scene's own. In the tracer's units, where a shade of 128 is one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneModelLight {
    /// Signed, as retail keeps it before clamping.
    pub ambient: [f32; 3],
    pub color: [f32; 3],
    /// Towards the light.
    pub direction: [f32; 3],
    /// How much of the light retail gave the model the tracer keeps, 0 to
    /// 1, the rest traced: the pick-ups' share, which retail lit in their
    /// own pulsing colour, unshadowed, so they read in any light
    /// (2026-10-23: far less affected by light).
    pub own: f32,
}

impl SceneModelLight {
    pub fn from_model(lighting: ModelLighting) -> Self {
        let directional = lighting.directional_word.to_le_bytes();
        Self {
            ambient: lighting.ambient.map(|channel| channel as f32 / 128.0),
            color: std::array::from_fn(|channel| f32::from(directional[channel]) / 128.0),
            direction: lighting.direction,
            own: 0.0,
        }
    }
}

/// The eye and its projection. `basis` holds the view's right, up and
/// forward axes in world space; a point at view-space `(x, y, z)` lands at
/// native pixel `centre + focal * (x, -y) / z`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneCamera {
    pub eye: [f64; 3],
    pub basis: [[f32; 3]; 3],
    pub focal: [f32; 2],
    pub centre: [f32; 2],
    /// The native frame the projection's pixels are in.
    pub native: [u32; 2],
    /// Near plane distance and the depth mapping's far distance.
    pub near: f32,
    pub far: f32,
}

/// The world's directional light, in the tracer's linear units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneSun {
    /// Towards the light, unit length.
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub ambient: [f32; 3],
}

/// A point light, eye-relative.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneLight {
    pub position: [f32; 3],
    pub radius: f32,
    pub color: [f32; 3],
}

/// A glowing model's light (the tuning's glows: the crystals), where its
/// blended faces gather: eye-relative, a face's sprite (an index into
/// [`SceneFeed::textures`] once traced) and colour, whose mean it shines
/// in, its strength and reach, how far apart its faces give lights of
/// their own (0: a quarter of its reach), and the model.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneGlow {
    pub position: [f32; 3],
    /// The other end of a line light (a laser beam), or none for a point.
    pub end: Option<[f32; 3]>,
    pub texture: Option<u32>,
    pub color: [u8; 4],
    pub strength: f32,
    pub reach: f32,
    pub gather: f32,
    pub model: u16,
    /// How many faces it gathers.
    pub faces: u32,
    /// The sprites its faces show, each with how many faces show it (the
    /// first four), so it shines in all of theirs whichever is drawn
    /// first.
    pub sprites: [(u32, u16); 4],
}

impl SceneGlow {
    /// One more face shows `texture`; past four sprites, the face counts
    /// without its colour.
    fn add_sprite(&mut self, texture: Option<u32>) {
        let Some(texture) = texture else {
            return;
        };
        if let Some(slot) = self
            .sprites
            .iter_mut()
            .find(|(sprite, count)| *count > 0 && *sprite == texture)
        {
            slot.1 = slot.1.saturating_add(1);
        } else if let Some(slot) = self.sprites.iter_mut().find(|(_, count)| *count == 0) {
            *slot = (texture, 1);
        }
    }
}

/// A vehicle's exhaust (the tuning's trails): where it is, eye-relative,
/// how it pushes its puffs (world units a second), and their look.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneExhaust {
    pub position: [f32; 3],
    pub push: [f32; 3],
    pub look: u8,
}

/// A blended sprite the raster adds over the traced world (a particle's,
/// a bolt's streak, a beam segment, a model's effect billboard), for
/// mirrored effects: where it starts or sits (eye-relative), its span to its
/// other end (`[0.0; 3]` for a round billboard), its world half-size, its
/// sprite (an index into [`SceneFeed::textures`] once traced) and the colour
/// it is drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneEmitter {
    pub position: [f32; 3],
    pub span: [f32; 3],
    pub radius: f32,
    pub texture: Option<u32>,
    pub color: [u8; 3],
}

/// A place the player's craft stirs a puddle from: eye-relative, and how
/// hard, 0 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneWake {
    pub position: [f32; 3],
    pub strength: f32,
}

/// A wheel model drawn by an object: the object (an index into
/// [`SceneFeed::objects`]), its hub (eye-relative, the wheel model's
/// origin) and how far its corners reach from it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneWheel {
    pub object: u32,
    pub hub: [f32; 3],
    pub radius: f32,
}

/// A splash on the water ([`crate::process_particles::splashes`]),
/// eye-relative, with its age (0 to 1) and scale.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneSplash {
    pub position: [f32; 3],
    pub age: f32,
    pub scale: f32,
}

/// A blended edge of a model whose arcs span a field (the active
/// warpgate's lightning, [`crate::scene_field`]): its ends (eye-relative),
/// and the field's strength and colour by the tuning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneArc {
    pub model: u16,
    pub ends: [[f32; 3]; 2],
    pub strength: f32,
    pub color: [f32; 3],
}

/// A vehicle's lamp ([`crate::world_scene::place_lamps`]): where it is
/// (eye-relative), the way its beam points, the beam's across and up, its
/// colour, and whether it is a rear lamp (its beam the tuning's rear one).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneLamp {
    pub position: [f32; 3],
    pub axis: [f32; 3],
    pub across: [f32; 3],
    pub up: [f32; 3],
    pub color: [f32; 3],
    pub rear: bool,
}

/// An arc of a gate the trace's frame redraws ([`crate::scene_gate`]): its
/// ends (eye-relative), retail's half-width for it, its sprite (an index
/// into the builder's sprites, whose colour it takes) and its gate's look
/// (an index into the tuning's `materials.gates`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneBolt {
    pub ends: [[f32; 3]; 2],
    pub half_width: f32,
    pub sprite: u32,
    pub look: usize,
}

/// A flare of such a gate, where retail's quad drew it: its middle in
/// native pixels, its view depth and its half-diagonal in native pixels,
/// with its sprite and its gate's look as a bolt's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneGlint {
    pub centre: [f32; 2],
    pub depth: f32,
    pub size: f32,
    pub sprite: u32,
    pub look: usize,
}

/// An explosion's fireball ([`crate::process_particles::splashes`]): where
/// it is (world units, X and Z wrapping at 16 bits), how far under the
/// water's surface (negative above it; none where no water lies under it),
/// and whether it is of the larger kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneBlast {
    pub world: [u16; 3],
    pub depth: Option<f32>,
    pub large: bool,
}

/// The world's fog: colour, and the distances it starts and is complete,
/// and the colour retail's far terrain fades into beyond it (H7 `0x73`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneFog {
    pub color: [f32; 3],
    pub start: f32,
    pub end: f32,
    pub far_color: [f32; 3],
    /// Retail draws its coarse far terrain out to 1.5 times the fog's end
    /// (a world's view), so the haze completes there; without it (the
    /// briefing's camera pane) it completes at the fog's end.
    pub far_ring: bool,
}

/// Retail's dynamic light grid ([`crate::world_lighting`]) as the frame's
/// draws left it: lamps, vehicles, particles and explosions deposit signed
/// RGB into it, and the tracer adds it as retail does (twice the value on
/// the ground; once in a model's ambient and once in its sun colour).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneLightGrid {
    /// World X and Z of cell (0, 0), as `0045B180` published them.
    pub anchors: [u16; 2],
    /// The terrain spacing: coarse ground (256) reads every other cell.
    pub spacing: u16,
    /// 90 by 90 cells of 128 units, X-major, Z-minor.
    pub cells: Vec<[i8; 3]>,
    /// The executable's colour beyond the grid (`00526068`).
    pub outside: [i8; 3],
    /// Settings +80: off, the ground takes no grid light (models still do).
    pub ground: bool,
    /// The lights deposited this frame, so the tracer can shadow them.
    pub sources: Vec<SceneLightSource>,
}

/// A deposited light: eye-relative, a little above the ground under its
/// X/Z (the grid knows no height), its reach and its colour (128 is one),
/// and where it shines from: the actor carrying it or the particle shedding
/// it, else its own place. A particle's light may be shadowed by anything;
/// an actor's light by nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneLightSource {
    pub position: [f32; 3],
    pub radius: f32,
    pub rgb: [f32; 3],
    pub particle: bool,
    pub emitter: [f32; 3],
    /// How much it warms what it lights under thermal vision: a particle's
    /// own colour's brightest channel; lamps, infrared's among them, none.
    pub heat: f32,
}

/// A raining or snowing world's weather in one frame: the share of its
/// tuned fall falling now (its storm), how wet the ground and the models
/// are, and how far its puddles have filled, 0 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneWeather {
    pub fall: f32,
    pub wet_ground: f32,
    pub wet_models: f32,
    pub puddles: f32,
}

/// One world frame's scene.
#[derive(Clone, Debug, Default)]
pub struct SceneFeed {
    /// How far night has fallen ([`crate::world_scene::nightfall`]), for
    /// the weather that waits for it.
    pub nightfall: f32,
    /// Whether that weather may fall here: Pelagia's rain is the rework's,
    /// so world 1 has it only with the rework on.
    pub night_weather: bool,
    /// A raining or snowing world's weather now ([`crate::world_weather`]);
    /// none keeps the fall and the sky as tuned.
    pub weather: Option<SceneWeather>,
    /// The sparks running through struck crystals now.
    pub sparks: Vec<crate::crystal_flares::CrystalSpark>,
    /// Where the player's craft stirs the puddles it drives through, from
    /// the eye, and how hard (0 to 1): each wheel on the ground, or its
    /// middle where it draws no wheels ([`crate::world_scene`]'s
    /// `craft_wakes`).
    pub wakes: Vec<SceneWake>,
    /// Where the craft's spray falls back on the puddles and rings them
    /// ([`crate::craft_spray`]): each eye-relative, at the water, and how
    /// far through its life it is (0 to 1), the nearest first.
    pub spray_rings: Vec<[f32; 4]>,
    /// The wheels drawn this frame ([`crate::scene_capture`]).
    pub wheels: Vec<SceneWheel>,
    pub camera: SceneCamera,
    pub vertices: Vec<SceneVertex>,
    pub triangles: Vec<SceneTriangle>,
    /// Texture content keys ([`crate::d3d_texture::CpuTexture::content_key`])
    /// and the sprite (an H3 slot) each was resolved from, where the trace
    /// knows it (else -1): the tuning's materials by sprite.
    pub textures: Vec<(u64, i16)>,
    pub sun: SceneSun,
    /// The world's first light, in the sun's units, which its light falls
    /// from (Artoria's generators); none follows nothing.
    pub light_base: Option<[f32; 3]>,
    pub lights: Vec<SceneLight>,
    pub fog: SceneFog,
    /// The frame's clear colour, the sky retail shows.
    pub sky: [f32; 3],
    /// The ground ([`crate::scene_terrain`]): the same mesh until the
    /// terrain changes, in world units rather than relative to the eye.
    pub terrain: Option<Rc<TerrainMesh>>,
    /// The same ground's grid for Vulkan's raster on a world the rain may
    /// wet, never drawn: the device uploads it ahead, and while the rain
    /// wets the ground ([`Self::raster_wet_ground`]) its wet ground and
    /// puddles read the heights and normals (the frame block's grid), where
    /// [`Self::terrain`] would take the place of retail's ground.
    pub raster_ground: Option<Rc<TerrainMesh>>,
    /// Each blend rank's texture, an index into [`Self::textures`] (none:
    /// untextured).
    pub terrain_textures: Vec<Option<u32>>,
    /// Retail's dynamic lights, complete once the world's draws ran.
    pub light_grid: Option<SceneLightGrid>,
    /// The lights models were shaded with ([`SceneTriangle::light`]).
    pub model_lights: Vec<SceneModelLight>,
    /// Whether the feed holds the water within sight
    /// ([`crate::scene_water`]): the raster then leaves its water strips.
    pub water: bool,
    /// The wave clock the water's heights were taken at.
    pub water_clock: u32,
    /// The game's clock in microseconds (the animation clock's), which the
    /// tracer's eruptions age by.
    pub clock_micros: u64,
    /// How far from the eye the water mesh reaches (on X and Z); beyond it
    /// the tracer takes the water's plane.
    pub water_reach: f32,
    /// The water's plane, in world height, where the world has water: the
    /// frame block's water level where no ground mesh brings it (Vulkan's
    /// raster, its rain stopping there).
    pub water_level: Option<f32>,
    /// Port option, the remastered camera: the eye is under the water's
    /// live surface (the world draw's own decision,
    /// [`crate::world_scene::WorldDrawReceipt::under_water`]), where the
    /// tracer lays the water round it as a medium; never over lava or under
    /// infrared vision.
    pub eye_under: bool,
    /// The water's texture, an index into [`Self::textures`].
    pub water_texture: Option<u32>,
    /// The water's own colour where no texture brings it (Vulkan's raster:
    /// its lamps' hue in the water).
    pub water_hue: Option<[f32; 3]>,
    /// The world's number (the mission's `+A0`), for its tuning.
    pub world: u32,
    /// How far a darkened world's fog has rolled in, 0 to 1
    /// ([`crate::modern_tuning::ModernTuning::fog_at`]).
    pub darkened: f32,
    /// The names of the models the triangles came from, by H8 slot, for
    /// their tuning (thermal vision's heat).
    pub model_names: crate::fast_hash::FastMap<u16, Box<str>>,
    /// Infrared vision is on.
    pub infrared: bool,
    /// The objects drawn ([`SceneTriangle::object`]).
    pub objects: Vec<SceneObject>,
    /// Where the guns' shots were born this frame (world units, wrapping
    /// at 16 bits): the bolts and the cannon's sparks, which warm the guns
    /// that fired them.
    pub shots: Vec<[u16; 3]>,
    /// Retail's splashes, for rings on the traced water.
    pub splashes: Vec<SceneSplash>,
    /// The explosions' fireballs, which throw the traced water up where
    /// they go off in it.
    pub blasts: Vec<SceneBlast>,
    /// The glowing models' lights.
    pub glows: Vec<SceneGlow>,
    /// The blended sprites that add light, which the water and metal
    /// mirror under Eye candy.
    pub emitters: Vec<SceneEmitter>,
    /// The arcs that span fields of energy.
    pub arcs: Vec<SceneArc>,
    /// The gates' arcs and flares the trace's frame redraws.
    pub bolts: Vec<SceneBolt>,
    pub glints: Vec<SceneGlint>,
    /// The vehicles' headlamps, which light in place of retail's pool.
    pub lamps: Vec<SceneLamp>,
    /// The vehicles' exhausts the tuning's trails name, as drawn, and where
    /// the vehicles stand whose players thrust them (both eye-relative):
    /// their exhausts leave trails ([`crate::scene_trails`]).
    pub exhausts: Vec<SceneExhaust>,
    pub thrusts: Vec<[f32; 3]>,
    /// The player's craft the camera follows (eye-relative), which the
    /// clouds keep in sight; none in first person, while the player steers
    /// the Cranial Implanter's missile, and without a world.
    pub craft: Option<[f32; 3]>,
    /// Its wake through the clouds, newest first: eye-relative points and
    /// their ages in seconds ([`crate::cloud_wake`]).
    pub cloud_wake: Vec<[f32; 4]>,
    /// The native pixels the traced view fills, inclusive (left, top, right,
    /// bottom): a pane such as the briefing's camera; none for the whole
    /// frame.
    pub clip: Option<[i16; 4]>,
    /// How far along a primary hit's normal its shadow rays start, past the
    /// half unit every ray takes: the traced menu's low-poly planets, lit
    /// along smooth normals, would otherwise shadow themselves in blocks.
    pub shadow_lift: f32,
}

impl SceneFeed {
    /// [`Self::raster_ground`] while the rain wets the ground.
    pub fn raster_wet_ground(&self) -> Option<&Rc<TerrainMesh>> {
        self.raster_ground
            .as_ref()
            .filter(|_| self.weather.is_some_and(|weather| weather.wet_ground > 0.0))
    }

    /// Begin a frame's feed for `camera`, keeping the allocations.
    pub fn begin(&mut self, camera: SceneCamera) {
        self.camera = camera;
        self.vertices.clear();
        self.triangles.clear();
        self.textures.clear();
        self.lights.clear();
        self.terrain = None;
        self.raster_ground = None;
        self.terrain_textures.clear();
        self.light_grid = None;
        self.model_lights.clear();
        self.water = false;
        self.water_reach = 0.0;
        self.water_level = None;
        self.eye_under = false;
        self.water_texture = None;
        self.water_hue = None;
        self.model_names.clear();
        self.objects.clear();
        self.wakes.clear();
        self.spray_rings.clear();
        self.wheels.clear();
        self.shots.clear();
        self.splashes.clear();
        self.blasts.clear();
        self.glows.clear();
        self.emitters.clear();
        self.arcs.clear();
        self.bolts.clear();
        self.glints.clear();
        self.lamps.clear();
        self.exhausts.clear();
        self.thrusts.clear();
        self.clip = None;
    }

    /// The feed index of a texture, added on first use.
    pub fn texture(&mut self, content_key: u64) -> u32 {
        match self
            .textures
            .iter()
            .position(|(key, _)| *key == content_key)
        {
            Some(index) => index as u32,
            None => {
                self.textures.push((content_key, -1));
                (self.textures.len() - 1) as u32
            }
        }
    }

    /// Add a polygon as a fan of triangles over `vertices`.
    pub fn polygon(
        &mut self,
        vertices: &[SceneVertex],
        texture: Option<u32>,
        surface: SceneSurface,
        model: Option<u16>,
        light: Option<u32>,
        cutout: bool,
    ) {
        if vertices.len() < 3 {
            return;
        }
        let first = self.vertices.len() as u32;
        self.vertices.extend_from_slice(vertices);
        for index in 1..vertices.len() as u32 - 1 {
            self.triangles.push(SceneTriangle {
                vertices: [first, first + index, first + index + 1],
                texture,
                surface,
                model,
                light,
                cutout,
                own_fog: false,
                object: None,
                culled: false,
            });
        }
    }
}

/// A feed a scene collects while it draws a world: the feed, and the H3
/// sprites its textured triangles name, with the variant the raster draws
/// them in. Until the trace resolves them, triangle textures index
/// [`Self::sprites`].
pub struct SceneFeedBuilder<'r> {
    pub feed: SceneFeed,
    pub sprites: Vec<(i16, RenderResource<'r>, SpriteRenderMode)>,
    /// The sprites (indices into `sprites`) that are the modern sky's land
    /// band, to trace with their painted sky cut away as that band says
    /// ([`crate::d3d_texture::cut_band`]).
    pub band_cuts: Vec<(u32, crate::modern_tuning::BandTuning)>,
    /// A billboard the modern sky keeps (the Moon), captured and waiting
    /// for its command's corners: its sprite index, its model and whether
    /// the sprite is masked.
    pub sky_billboard: Option<(u32, u16, bool)>,
    /// An additive billboard sunk under the water, captured and waiting
    /// for its command's corners, which the tracer takes as a
    /// [`SceneSurface::Glow`] so the water shows it (its sprite index and
    /// its model); the raster leaves the command it places.
    pub sunk_billboard: Option<(u32, u16)>,
    /// A gate's flare, captured and waiting for its command's corners: its
    /// sprite index and its gate's look.
    pub gate_glint: Option<(u32, usize)>,
    /// The primitive being prepared was added to [`SceneSurface::Glow`] in 3D,
    /// while the raster still draws it for the primary view.
    pub glow_fed: bool,
    /// Each terrain blend rank's sprite index, until the trace resolves
    /// them into [`SceneFeed::terrain_textures`].
    pub terrain_sprites: Vec<Option<u32>>,
    /// The water's sprite index, until the trace resolves it into
    /// [`SceneFeed::water_texture`].
    pub water_sprite: Option<u32>,
    /// The display's material channel shifts, for flat colours.
    pub material_shifts: [u32; 3],
    /// Each model face this frame by its exact corners: the faces drawn
    /// over those corners, the raster's last one to stay
    /// ([`Self::settle_faces`]).
    faces: crate::fast_hash::FastMap<u64, FaceChain>,
    /// Every face remembered this frame, in order; each chain's faces link
    /// on through [`FeedFace::next`].
    face_list: Vec<FeedFace>,
    /// The faces the primitive being prepared added (places in
    /// `face_list`), until the raster submits it.
    pending: Vec<usize>,
    /// Each sprite's index in [`Self::sprites`] by its id and mode.
    sprite_index: crate::fast_hash::FastMap<(i16, std::mem::Discriminant<SpriteRenderMode>), u32>,
    /// Counts the raster's submissions of fed primitives.
    submissions: u32,
    /// The models take only the tracer's light: their faces keep neither
    /// retail's shade nor the light it shaded them with (the traced main
    /// menu, its planets lit from its sun).
    pub lit_by_trace: bool,
    /// The water's plane, eye-relative, where the world has water.
    pub water_plane: Option<f32>,
    /// The share of retail's light the models drawn now keep
    /// ([`SceneModelLight::own`]): a pick-up's while it draws, else 0.
    pub own_light: f32,
    /// The models drawn now are fogged by their draw's own fog
    /// ([`SceneTriangle::own_fog`]).
    pub own_fog: bool,
    /// The world's fade and far colour as its draw published them, which a
    /// model's draw may replace with its own (the actor flash).
    pub world_fog: Option<([i32; 2], u32)>,
    /// Where drawn objects' vertices were in the last traced frame, lent by
    /// the frame for this feed.
    pub motion: MotionHistory,
    /// The object whose models are drawing, with the models it has drawn
    /// so far this frame.
    drawing: ObjectDraws,
    /// The motion key of the last model and transform looked up: a draw's
    /// vertices come one after another.
    motion_key: Option<(u16, crate::model_context::ModelTransform, Option<MotionKey>)>,
    /// That object's index in [`SceneFeed::objects`].
    object: Option<u32>,
    /// The height of the picture the trace is shown at, in pixels, which
    /// round parts are split for (0: the camera's native frame).
    pub target_height: f32,
    /// Each model this frame by its global index: the round part it is, if
    /// any ([`crate::round_parts`]).
    pub(crate) round_parts:
        crate::fast_hash::FastMap<u16, Option<std::rc::Rc<crate::round_parts::RoundModel>>>,
    /// Each tread sprite this frame: its brightness column by column, from
    /// its darkest (0) to its brightest (1), for the treads' lugs.
    pub(crate) tread_profiles: crate::fast_hash::FastMap<i16, Option<std::rc::Rc<[f32]>>>,
    /// Each model this frame by its global index: whether the tuning gives
    /// it exhausts ([`crate::scene_capture`]'s `trail_exhausts`).
    pub(crate) exhaust_models: crate::fast_hash::FastMap<u16, bool>,
    /// A round part's split corners, kept from draw to draw.
    pub(crate) split_corners: Vec<SceneVertex>,
    /// The models drawing now are for the tracer alone, outside the view
    /// ([`crate::software_scene::SoftwareScene::feed_only`]): their faces
    /// are [`SceneTriangle::culled`].
    pub culled: bool,
}

/// A model face's key: its exact corners, in any order.
fn face_key(corners: &[SceneVertex]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = crate::fast_hash::FxHasher::default();
    let mut inline = [[0_u32; 3]; 16];
    let mut spilled = Vec::new();
    let positions: &mut [[u32; 3]] = if corners.len() <= inline.len() {
        &mut inline[..corners.len()]
    } else {
        spilled.resize(corners.len(), [0; 3]);
        &mut spilled
    };
    for (position, corner) in positions.iter_mut().zip(corners) {
        *position = corner.position.map(f32::to_bits);
    }
    positions.sort_unstable();
    positions.hash(&mut hasher);
    hasher.finish()
}

/// The first and last of the faces remembered over one set of corners, as
/// places in [`SceneFeedBuilder::face_list`], and how many.
#[derive(Clone, Copy, Debug)]
struct FaceChain {
    first: usize,
    last: usize,
    count: usize,
}

/// A model face in the feed: its triangles, and where the raster's queue
/// drew it (its sort key, as the queue's signed depth, and its
/// submission), or none when the raster drew nothing of it this frame
/// (culled or clipped away).
#[derive(Clone, Debug)]
struct FeedFace {
    triangles: std::ops::Range<usize>,
    drawn: Option<(i32, u32)>,
    /// The next face over the same corners, if any.
    next: Option<usize>,
    /// Drawn for the tracer alone ([`SceneFeedBuilder::culled`]).
    culled: bool,
}

impl FeedFace {
    /// Whether the raster draws this face after `other`: its queue replays
    /// a sorted group by descending signed key, equal keys in submission
    /// order (`draw_queue.rs`), and a face it never drew comes first. A
    /// face drawn for the tracer alone never wins over the view's: the
    /// raster drew none of it.
    fn drawn_after(&self, other: &FeedFace) -> bool {
        if self.culled != other.culled {
            return other.culled;
        }
        match (self.drawn, other.drawn) {
            (Some((key, order)), Some((other_key, other_order))) => {
                key < other_key || (key == other_key && order > other_order)
            }
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => self.triangles.start > other.triangles.start,
        }
    }
}

impl<'r> SceneFeedBuilder<'r> {
    pub fn new(camera: SceneCamera, material_shifts: [u32; 3]) -> Self {
        let mut feed = SceneFeed::default();
        feed.begin(camera);
        Self {
            feed,
            sprites: Vec::new(),
            band_cuts: Vec::new(),
            sky_billboard: None,
            sunk_billboard: None,
            gate_glint: None,
            glow_fed: false,
            terrain_sprites: Vec::new(),
            water_sprite: None,
            material_shifts,
            faces: Default::default(),
            face_list: Vec::new(),
            pending: Vec::new(),
            sprite_index: Default::default(),
            submissions: 0,
            lit_by_trace: false,
            water_plane: None,
            own_light: 0.0,
            own_fog: false,
            world_fog: None,
            motion: MotionHistory::default(),
            drawing: ObjectDraws::default(),
            motion_key: None,
            object: None,
            target_height: 0.0,
            round_parts: Default::default(),
            exhaust_models: Default::default(),
            split_corners: Vec::new(),
            tread_profiles: Default::default(),
            culled: false,
        }
    }

    /// `object`'s models start drawing, from model `root` (and its name,
    /// which a root that draws only children leaves the feed without) at
    /// world `position`: their vertices take its motion, their faces its
    /// heat.
    pub fn begin_object(
        &mut self,
        object: crate::object_runtime::ObjectId,
        position: [i32; 3],
        root: Option<u16>,
        name: Option<&[u8]>,
    ) {
        if let (Some(root), Some(name)) = (root, name) {
            self.feed
                .model_names
                .entry(root)
                .or_insert_with(|| String::from_utf8_lossy(name).into());
        }
        self.drawing.begin(object);
        self.motion_key = None;
        self.object = Some(self.feed.objects.len() as u32);
        self.feed.objects.push(SceneObject {
            id: object,
            position,
            root,
        });
    }

    /// A corner (eye-relative) of a wheel model whose hub, its origin, is
    /// at `hub`, drawn by the object drawing now: the wheel is kept once per
    /// hub, reaching as far as its farthest corner.
    pub fn wheel_corner(&mut self, hub: [f32; 3], corner: [f32; 3]) {
        let Some(object) = self.object else {
            return;
        };
        let reach = (0..3)
            .map(|axis| (corner[axis] - hub[axis]).powi(2))
            .sum::<f32>()
            .sqrt();
        match self
            .feed
            .wheels
            .iter_mut()
            .find(|wheel| wheel.object == object && wheel.hub == hub)
        {
            Some(wheel) => wheel.radius = wheel.radius.max(reach),
            None => self.feed.wheels.push(SceneWheel {
                object,
                hub,
                radius: reach,
            }),
        }
    }

    /// The object's models are drawn.
    pub fn end_object(&mut self) {
        self.drawing.end();
        self.motion_key = None;
        self.object = None;
    }

    /// The motion of vertex slot `slot` of model `model`, drawn in
    /// `transform` by the object drawing now, at eye-relative `position`:
    /// zero outside an object's draw and on a vertex's first frame.
    pub fn vertex_motion(
        &mut self,
        model: u16,
        transform: crate::model_context::ModelTransform,
        slot: u16,
        position: [f32; 3],
    ) -> [f32; 3] {
        let key = match self.motion_key {
            Some((known, at, key)) if known == model && at == transform => key,
            _ => {
                let key = self.drawing.key(model, transform);
                self.motion_key = Some((model, transform, key));
                key
            }
        };
        let Some(key) = key else {
            return [0.0; 3];
        };
        let eye = self.feed.camera.eye;
        let at = std::array::from_fn(|axis| eye[axis] + f64::from(position[axis]));
        self.motion.displacement(key, usize::from(slot), at)
    }

    /// Add a model's face as [`SceneFeed::polygon`] does, remembered by its
    /// exact corners. Retail's models draw some faces twice over the same
    /// corners, in different sprites (the hovercraft's fans). The raster's
    /// depth test lets whichever it draws last win that tie, where a ray
    /// would take either from frame to frame; [`Self::settle_faces`] keeps
    /// that one.
    #[allow(clippy::too_many_arguments)]
    pub fn face(
        &mut self,
        corners: &[SceneVertex],
        texture: Option<u32>,
        surface: SceneSurface,
        model: Option<u16>,
        light: Option<u32>,
        cutout: bool,
    ) {
        let first = self.feed.triangles.len();
        self.feed
            .polygon(corners, texture, surface, model, light, cutout);
        self.remember(face_key(corners), first, surface);
    }

    /// Add a model's face split onto its round part's shape
    /// ([`crate::round_parts`]): `vertices` and the `triangles` over them,
    /// remembered by the face's own `corners` as [`Self::face`] remembers
    /// a face.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn split_face(
        &mut self,
        corners: &[SceneVertex],
        vertices: &[SceneVertex],
        triangles: &[[u32; 3]],
        texture: Option<u32>,
        surface: SceneSurface,
        model: Option<u16>,
        light: Option<u32>,
        cutout: bool,
    ) {
        let first = self.feed.triangles.len();
        let base = self.feed.vertices.len() as u32;
        self.feed.vertices.extend_from_slice(vertices);
        for triangle in triangles {
            self.feed.triangles.push(SceneTriangle {
                vertices: triangle.map(|index| base + index),
                texture,
                surface,
                model,
                light,
                cutout,
                own_fog: false,
                object: None,
                culled: false,
            });
        }
        self.remember(face_key(corners), first, surface);
    }

    /// The triangles from `first` on are a face keyed `key`, drawn by the
    /// object drawing now under its fog. Additive glow layers do not compete
    /// with opaque faces drawn over the same corners.
    fn remember(&mut self, key: u64, first: usize, surface: SceneSurface) {
        for triangle in &mut self.feed.triangles[first..] {
            triangle.own_fog = self.own_fog;
            triangle.object = self.object;
        }
        if surface == SceneSurface::Glow {
            return;
        }
        let place = self.face_list.len();
        self.face_list.push(FeedFace {
            triangles: first..self.feed.triangles.len(),
            drawn: None,
            next: None,
            culled: self.culled,
        });
        match self.faces.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut chain) => {
                let chain = chain.get_mut();
                self.face_list[chain.last].next = Some(place);
                chain.last = place;
                chain.count += 1;
            }
            std::collections::hash_map::Entry::Vacant(chain) => {
                chain.insert(FaceChain {
                    first: place,
                    last: place,
                    count: 1,
                });
            }
        }
        self.pending.push(place);
    }

    /// A strip `half_width` to each side of the line from `ends[0]` to
    /// `ends[1]` (eye-relative world positions), turned to face the eye as
    /// retail's edges face the screen, its texture's `v` running across it
    /// from 0 to 1, as [`Self::face`] adds faces. It is two faces meeting
    /// along the line, whose normals turn from one side through the eye to
    /// the other, so the tracer lights it as the rod it stands for. `corner`
    /// gives each end's corners all but their position, normal and `v`.
    #[allow(clippy::too_many_arguments)]
    pub fn ribbon(
        &mut self,
        ends: [[f32; 3]; 2],
        half_width: f32,
        corner: [SceneVertex; 2],
        texture: Option<u32>,
        model: Option<u16>,
        light: Option<u32>,
        cutout: bool,
    ) {
        let unit = |v: [f32; 3]| {
            let length = v.iter().map(|c| c * c).sum::<f32>().sqrt();
            (length > 1.0e-6).then(|| v.map(|c| c / length))
        };
        let cross = |a: [f32; 3], b: [f32; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let along: [f32; 3] = std::array::from_fn(|i| ends[1][i] - ends[0][i]);
        let middle: [f32; 3] = std::array::from_fn(|i| -(ends[0][i] + ends[1][i]) * 0.5);
        let (Some(along), Some(to_eye)) = (unit(along), unit(middle)) else {
            return;
        };
        // A line pointing at the eye shows no width.
        let Some(side) = unit(cross(along, to_eye)) else {
            return;
        };
        if half_width <= 0.0 {
            return;
        }
        let facing = cross(side, along);
        let at = |end: usize, across: f32, normal: [f32; 3], v: f32| SceneVertex {
            position: std::array::from_fn(|i| ends[end][i] + side[i] * across),
            normal,
            uv: [corner[end].uv[0], v],
            ..corner[end]
        };
        let back = side.map(|c| -c);
        for (outer, normal, v) in [(half_width, side, 0.0), (-half_width, back, 1.0)] {
            self.face(
                &[
                    at(0, outer, normal, v),
                    at(0, 0.0, facing, 0.5),
                    at(1, 0.0, facing, 0.5),
                    at(1, outer, normal, v),
                ],
                texture,
                SceneSurface::Opaque,
                model,
                light,
                cutout,
            );
        }
    }

    /// Gather a glowing model's face centred at `centre` into its light: the
    /// model's nearest within its gather (a quarter of its reach unless the
    /// tuning says), else a new one.
    pub fn glow(&mut self, centre: [f32; 3], light: SceneGlow) {
        let gather = if light.gather > 0.0 {
            light.gather
        } else {
            light.reach * 0.25
        };
        let near = |glow: &&mut SceneGlow| {
            let apart: f32 = (0..3).map(|i| (glow.position[i] - centre[i]).powi(2)).sum();
            glow.model == light.model && apart < gather.powi(2)
        };
        match self.feed.glows.iter_mut().find(near) {
            Some(glow) => {
                glow.faces += 1;
                glow.add_sprite(light.texture);
                let share = 1.0 / glow.faces as f32;
                for (axis, value) in glow.position.iter_mut().enumerate() {
                    *value += (centre[axis] - *value) * share;
                }
            }
            None => self.lone_glow(centre, light),
        }
    }

    /// A vehicle's exhaust, once however many of its model's primitives lay
    /// it.
    pub fn exhaust(&mut self, exhaust: SceneExhaust) {
        let laid = |other: &SceneExhaust| {
            other.look == exhaust.look
                && (0..3).all(|axis| (other.position[axis] - exhaust.position[axis]).abs() < 1.0)
        };
        if !self.feed.exhausts.iter().any(laid) {
            self.feed.exhausts.push(exhaust);
        }
    }

    /// A light of its own at `centre`, gathering nothing: the guns' bolts,
    /// whose twin guns fire side by side.
    pub fn lone_glow(&mut self, centre: [f32; 3], light: SceneGlow) {
        let mut glow = SceneGlow {
            position: centre,
            faces: 1,
            sprites: Default::default(),
            ..light
        };
        glow.add_sprite(light.texture);
        self.feed.glows.push(glow);
    }

    /// A new primitive is being captured: the last one's faces wait no more.
    pub fn begin_primitive(&mut self) {
        self.pending.clear();
        self.sky_billboard = None;
        self.sunk_billboard = None;
        self.gate_glint = None;
        self.glow_fed = false;
    }

    /// The waiting gate flare ([`Self::gate_glint`]) at the corners its
    /// command drew: native screen points and their view depths.
    pub fn place_gate_glint(&mut self, corners: [([i32; 2], i32); 4]) {
        let Some((sprite, look)) = self.gate_glint.take() else {
            return;
        };
        let centre =
            [0, 1].map(|axis| corners.iter().map(|(xy, _)| xy[axis] as f32).sum::<f32>() / 4.0);
        let size = corners
            .iter()
            .map(|(xy, _)| (xy[0] as f32 - centre[0]).hypot(xy[1] as f32 - centre[1]))
            .fold(0.0, f32::max);
        let depth = corners.iter().map(|&(_, depth)| depth as f32).sum::<f32>() / 4.0;
        self.feed.glints.push(SceneGlint {
            centre,
            depth,
            size,
            sprite,
            look,
        });
    }

    /// The waiting sky billboard ([`Self::sky_billboard`]) at the corners
    /// its command drew: native screen points and their view depths, from
    /// the top left round, as a sky face for the tracer to add to its sky.
    pub fn place_sky_billboard(&mut self, corners: [([i32; 2], i32); 4]) {
        let Some((sprite, model, masked)) = self.sky_billboard.take() else {
            return;
        };
        let camera = self.feed.camera;
        let [right, up, forward] = camera.basis;
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let vertices: Vec<SceneVertex> = corners
            .iter()
            .zip(uvs)
            .map(|(&([x, y], depth), uv)| {
                let z = depth as f32;
                let across = (x as f32 - camera.centre[0]) * z / camera.focal[0];
                let rise = -(y as f32 - camera.centre[1]) * z / camera.focal[1];
                SceneVertex {
                    position: std::array::from_fn(|axis| {
                        right[axis] * across + up[axis] * rise + forward[axis] * z
                    }),
                    normal: [0.0; 3],
                    uv,
                    color: [255; 4],
                    shade: [0; 4],
                    fog: [0; 4],
                    motion: [0.0; 3],
                }
            })
            .collect();
        self.face(
            &vertices,
            Some(sprite),
            SceneSurface::Sky,
            Some(model),
            None,
            masked,
        );
    }

    /// The waiting sunk billboard ([`Self::sunk_billboard`]) at the corners
    /// its command drew: native screen points and their view depths, from
    /// the top left round, in the quad's `color`, as the tracer's own
    /// [`SceneSurface::Glow`], which the water shows through it. True when
    /// it was placed, so the raster leaves the command's own draw; a corner
    /// behind the eye places nothing and the raster draws it as ever.
    pub fn place_sunk_billboard(&mut self, corners: [([i32; 2], i32); 4], color: [u8; 3]) -> bool {
        let Some((sprite, model)) = self.sunk_billboard.take() else {
            return false;
        };
        // A corner behind the eye has no world place to take.
        if corners.iter().any(|(_, depth)| *depth <= 0) {
            return false;
        }
        let camera = self.feed.camera;
        let [right, up, forward] = camera.basis;
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let vertices: Vec<SceneVertex> = corners
            .iter()
            .zip(uvs)
            .map(|(&([x, y], depth), uv)| {
                let z = depth as f32;
                let across = (x as f32 - camera.centre[0]) * z / camera.focal[0];
                let rise = -(y as f32 - camera.centre[1]) * z / camera.focal[1];
                SceneVertex {
                    position: std::array::from_fn(|axis| {
                        right[axis] * across + up[axis] * rise + forward[axis] * z
                    }),
                    normal: [0.0; 3],
                    uv,
                    color: [color[0], color[1], color[2], 255],
                    shade: [0; 4],
                    fog: [0; 4],
                    motion: [0.0; 3],
                }
            })
            .collect();
        self.face(
            &vertices,
            Some(sprite),
            SceneSurface::Glow,
            Some(model),
            None,
            false,
        );
        true
    }

    /// The raster submitted the primitive just captured, with queue key
    /// `depth`: its faces are drawn in that place (the first submission
    /// counts, for a primitive split by clipping or mirrored).
    pub fn submitted(&mut self, depth: u32) {
        self.submissions = self.submissions.wrapping_add(1);
        for place in self.pending.drain(..) {
            if let Some(face) = self.face_list.get_mut(place) {
                face.drawn = Some((depth as i32, self.submissions));
            }
        }
    }

    /// Of the faces drawn over the same corners, keep the one the raster
    /// drew last; the others' triangles shrink to nothing.
    pub fn settle_faces(&mut self) {
        self.pending.clear();
        let list = &self.face_list;
        let chain_of =
            |first: usize| std::iter::successors(Some(first), move |&place| list[place].next);
        for chain in self.faces.values() {
            if chain.count < 2 {
                continue;
            }
            let winner = chain_of(chain.first)
                .reduce(|best, next| {
                    if list[next].drawn_after(&list[best]) {
                        next
                    } else {
                        best
                    }
                })
                .expect("two or more faces");
            for place in chain_of(chain.first) {
                if place == winner {
                    continue;
                }
                for triangle in &mut self.feed.triangles[list[place].triangles.clone()] {
                    let [first, ..] = triangle.vertices;
                    triangle.vertices = [first; 3];
                }
            }
        }
        self.faces.clear();
        self.face_list.clear();
    }

    /// The sprite index of H3 `id` in `mode`, added on first use.
    pub fn sprite(&mut self, id: i16, resource: RenderResource<'r>, mode: SpriteRenderMode) -> u32 {
        let sprites = &mut self.sprites;
        *self
            .sprite_index
            .entry((id, std::mem::discriminant(&mode)))
            .or_insert_with(|| {
                sprites.push((id, resource, mode));
                (sprites.len() - 1) as u32
            })
    }

    /// Mark sprite `index` as `band`'s land.
    pub fn cut_band(&mut self, index: u32, band: crate::modern_tuning::BandTuning) {
        if !self.band_cuts.iter().any(|(known, _)| *known == index) {
            self.band_cuts.push((index, band));
        }
    }

    /// The index of `light` in the feed's model lights, added on first use
    /// (a model's primitives share one, so the last is checked first), with
    /// the share of it the models drawn now keep.
    pub fn model_light(&mut self, light: SceneModelLight) -> u32 {
        let light = SceneModelLight {
            own: self.own_light,
            ..light
        };
        let lights = &mut self.feed.model_lights;
        if lights.last() != Some(&light) {
            match lights.iter().position(|known| *known == light) {
                Some(index) => return index as u32,
                None => lights.push(light),
            }
        }
        (lights.len() - 1) as u32
    }

    /// A view-space point (the camera frame the game projects in) as an
    /// eye-relative world position: the basis rows are orthonormal, so the
    /// inverse is the transpose.
    pub fn world<T: Into<f64> + Copy>(&self, view: [T; 3]) -> [f32; 3] {
        let [right, up, forward] = self.feed.camera.basis;
        let [x, y, z] = view.map(|value| value.into() as f32);
        std::array::from_fn(|axis| right[axis] * x + up[axis] * y + forward[axis] * z)
    }

    /// A raw H7 material word as RGB, as the flat consumers convert it
    /// (`004C13B9`): display-dependent shifts, the blue one on the low byte.
    pub fn material_rgb(&self, material: u32) -> [u8; 3] {
        material_rgb(material, self.material_shifts)
    }
}

/// A raw H7 material word as RGB under the display's `shifts`, as the flat
/// consumers convert it (`004C13B9`).
pub(crate) fn material_rgb(material: u32, shifts: [u32; 3]) -> [u8; 3] {
    [
        ((material >> (shifts[0].wrapping_add(1) & 31)) & 0xf8) as u8,
        ((material >> (shifts[1].wrapping_add(1) & 31)) & 0xf8) as u8,
        (u32::from(material as u8) << (shifts[2].wrapping_sub(1) & 31) & 0xf8) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_polygon_becomes_a_fan_and_textures_are_shared() {
        let mut feed = SceneFeed::default();
        feed.begin(SceneCamera::default());
        let texture = feed.texture(42);
        assert_eq!(feed.texture(7), 1);
        assert_eq!(feed.texture(42), texture);
        let vertex = SceneVertex::default();
        feed.polygon(
            &[vertex; 5],
            Some(texture),
            SceneSurface::Opaque,
            Some(3),
            Some(0),
            true,
        );
        let fans: Vec<[u32; 3]> = feed.triangles.iter().map(|t| t.vertices).collect();
        assert_eq!(fans, [[0, 1, 2], [0, 2, 3], [0, 3, 4]]);
        assert!(feed.triangles.iter().all(|t| t.cutout));
        feed.polygon(&[vertex; 2], None, SceneSurface::Opaque, None, None, false);
        assert_eq!(feed.triangles.len(), 3);
        feed.begin(SceneCamera::default());
        assert!(feed.vertices.is_empty() && feed.textures.is_empty());
    }

    /// The view frame's transpose takes a view-space point back to the
    /// eye-relative world, and flat colours convert as the consumers do.
    #[test]
    fn view_points_and_materials_convert() {
        let camera = SceneCamera {
            basis: [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
            ..SceneCamera::default()
        };
        // RGB555: red shifts right 7 into 0xF8, green 2, blue left 2.
        let builder = SceneFeedBuilder::new(camera, [6, 1, 3]);
        // Right is world -Z and forward world +X: 100 ahead, 20 right.
        assert_eq!(builder.world([20, 0, 100]), [100.0, 0.0, -20.0]);
        assert_eq!(builder.material_rgb(0x7fff), [0xf8, 0xf8, 0xf8]);
        assert_eq!(builder.material_rgb(0x7c00), [0xf8, 0, 0]);
    }

    /// Faces drawn twice over the same corners keep the one the raster's
    /// queue draws last: the smaller signed key, then the later submission,
    /// and a face it drew over one it never drew; of two it never drew, the
    /// later. One drawn for the tracer alone (outside the view) never wins
    /// over the view's, though submitted after it. The others' triangles
    /// shrink to nothing; a face alone stays.
    #[test]
    fn faces_drawn_twice_keep_the_rasters_last() {
        let mut builder = SceneFeedBuilder::new(SceneCamera::default(), [6, 1, 3]);
        let corner = |x: f32, z: f32| SceneVertex {
            position: [x, 0.0, z],
            ..SceneVertex::default()
        };
        let quad = |x: f32| {
            [
                corner(x, 0.0),
                corner(x + 1.0, 0.0),
                corner(x + 1.0, 1.0),
                corner(x, 1.0),
            ]
        };
        // Each face: where, a texture to tell it by, and the queue key the
        // raster submitted it at, if it did.
        let faces: [(f32, u32, Option<u32>); 13] = [
            (0.0, 1, Some(100)),
            (0.0, 2, Some(100)),
            (10.0, 3, Some(100)),
            (10.0, 4, Some(200)),
            (20.0, 5, Some(0x4000_0000)),
            (20.0, 6, Some(0xc000_0000)),
            (30.0, 7, None),
            (30.0, 8, Some(500)),
            (40.0, 9, Some(1)),
            (50.0, 10, None),
            (50.0, 11, None),
            (60.0, 12, Some(100)),
            (60.0, 13, Some(100)),
        ];
        for (x, texture, key) in faces {
            builder.begin_primitive();
            builder.culled = texture == 13;
            let corners = quad(x);
            // The same corners in another order are the same face.
            let corners: Vec<_> = if texture % 2 == 0 {
                corners.iter().rev().copied().collect()
            } else {
                corners.to_vec()
            };
            builder.face(
                &corners,
                Some(texture),
                SceneSurface::Opaque,
                Some(1),
                None,
                texture == 2,
            );
            if let Some(key) = key {
                builder.submitted(key);
            }
        }
        builder.settle_faces();
        let mut kept: Vec<u32> = builder
            .feed
            .triangles
            .iter()
            .filter(|t| t.vertices[0] != t.vertices[1] || t.vertices[1] != t.vertices[2])
            .map(|t| t.texture.unwrap())
            .collect();
        kept.dedup();
        assert_eq!(kept, [2, 3, 6, 8, 9, 11, 12]);
        let masked: Vec<bool> = builder
            .feed
            .triangles
            .iter()
            .filter(|t| t.texture == Some(2))
            .map(|t| t.cutout)
            .collect();
        assert_eq!(masked, [true, true]);
    }

    /// A ribbon faces the eye: its corners stand its half width to each
    /// side of its line, across the view, with `v` 0 and 1 there and 0.5
    /// on the line, whose normals face the eye; two faces of two triangles.
    /// A line pointing at the eye, or of no length or width, adds nothing.
    #[test]
    fn a_ribbon_faces_the_eye_across_its_line() {
        let mut builder = SceneFeedBuilder::new(SceneCamera::default(), [6, 1, 3]);
        let corner = |u: f32| SceneVertex {
            uv: [u, 0.0],
            ..SceneVertex::default()
        };
        // An upright line 100 ahead on +Z.
        let ends = [[0.0, 0.0, 100.0], [0.0, 50.0, 100.0]];
        builder.ribbon(
            ends,
            4.0,
            [corner(0.0), corner(1.0)],
            Some(3),
            Some(7),
            None,
            true,
        );
        assert_eq!(builder.feed.triangles.len(), 4);
        let mut sides = Vec::new();
        for vertex in &builder.feed.vertices {
            let [x, y, z] = vertex.position;
            assert_eq!(z, 100.0, "across the view: {vertex:?}");
            assert_eq!(vertex.uv[0], y / 50.0);
            match vertex.uv[1] {
                v if v == 0.5 => {
                    assert_eq!(x, 0.0);
                    assert_eq!(vertex.normal, [0.0, 0.0, -1.0]);
                }
                v => {
                    assert_eq!(x.abs(), 4.0);
                    assert_eq!(vertex.normal, [x / 4.0, 0.0, 0.0]);
                    // One side is v 0, the other v 1.
                    sides.push((x > 0.0, v));
                }
            }
        }
        sides.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sides.dedup();
        assert_eq!(sides.len(), 2);
        assert_ne!(sides[0].1, sides[1].1);
        assert!(builder
            .feed
            .triangles
            .iter()
            .all(|t| t.cutout && t.model == Some(7)));
        let at_eye = [[0.0, 0.0, 10.0], [0.0, 0.0, 20.0]];
        builder.ribbon(at_eye, 4.0, [corner(0.0); 2], None, None, None, false);
        builder.ribbon([ends[0]; 2], 4.0, [corner(0.0); 2], None, None, None, false);
        builder.ribbon(ends, 0.0, [corner(0.0); 2], None, None, None, false);
        assert_eq!(builder.feed.triangles.len(), 4);
    }

    /// A glowing model's faces gather into one light per cluster, at their
    /// mean; another model's, or faces past a quarter of its reach, make
    /// their own.
    #[test]
    fn glowing_faces_gather_into_lights() {
        let mut builder = SceneFeedBuilder::new(SceneCamera::default(), [6, 1, 3]);
        let light = SceneGlow {
            strength: 0.5,
            reach: 400.0,
            model: 7,
            ..SceneGlow::default()
        };
        builder.glow([0.0, 0.0, 0.0], light);
        builder.glow([20.0, 40.0, 0.0], light);
        builder.glow([500.0, 0.0, 0.0], light);
        builder.glow([0.0, 0.0, 10.0], SceneGlow { model: 8, ..light });
        // A cluster keeps every sprite its faces show, however they come.
        let sprite = |texture| SceneGlow {
            texture: Some(texture),
            ..light
        };
        builder.glow([500.0, 10.0, 0.0], sprite(4));
        builder.glow([500.0, 20.0, 0.0], sprite(5));
        builder.glow([500.0, 30.0, 0.0], sprite(4));
        let glows = &builder.feed.glows;
        assert_eq!(glows.len(), 3);
        assert_eq!((glows[0].position, glows[0].faces), ([10.0, 20.0, 0.0], 2));
        assert_eq!(glows[1].faces, 4);
        assert_eq!(glows[1].sprites, [(4, 2), (5, 1), (0, 0), (0, 0)]);
        assert_eq!(glows[2].model, 8);
        // A glow that gathers nearer keeps faces 92 apart (the VTOL's two
        // jets) as lights of their own.
        let jets = SceneGlow {
            gather: 32.0,
            model: 9,
            ..light
        };
        builder.glow([0.0, 0.0, 0.0], jets);
        builder.glow([65.0, 0.0, 65.0], jets);
        builder.glow([65.0, 0.0, 65.0], jets);
        assert_eq!(builder.feed.glows.len(), 5);
        assert_eq!(builder.feed.glows[4].faces, 2);
    }

    /// A vertex of an object's model reports where it was in the last
    /// frame, its second draw of the same model keeps its own history, a
    /// jump past the most motion starts again, and models drawn outside an
    /// object's draw keep still.
    #[test]
    fn an_objects_vertices_report_their_own_motion() {
        let object = crate::object_runtime::ObjectArena::new()
            .allocate(crate::object_runtime::ObjectClassKind::VisualBase);
        let transform = |x| crate::model_context::ModelTransform {
            view_position: [x, 0, 0],
            view_basis: crate::menu::MenuBasis::INITIAL,
        };
        let mut history = MotionHistory::default();
        for (frame, x) in [(0, 0.0), (1, 30.0), (2, 4000.0)] {
            let mut builder = SceneFeedBuilder::new(SceneCamera::default(), [6, 1, 3]);
            builder.motion = std::mem::take(&mut history);
            assert_eq!(
                builder.vertex_motion(7, transform(0), 2, [x, 0.0, 0.0]),
                [0.0; 3],
                "outside an object's draw"
            );
            builder.begin_object(object, [0; 3], Some(7), None);
            let first = builder.vertex_motion(7, transform(0), 2, [x, 0.0, 0.0]);
            let second = builder.vertex_motion(7, transform(64), 2, [x + 100.0, 5.0, 0.0]);
            builder.end_object();
            match frame {
                0 => assert_eq!((first, second), ([0.0; 3], [0.0; 3])),
                1 => assert_eq!((first, second), ([-30.0, 0.0, 0.0], [-30.0, 0.0, 0.0])),
                _ => assert_eq!((first, second), ([0.0; 3], [0.0; 3]), "a jump starts again"),
            }
            builder.motion.finish_frame();
            history = builder.motion;
        }
    }

    /// Models shaded alike share one light; a new light is added.
    #[test]
    fn model_lights_are_shared() {
        let mut builder = SceneFeedBuilder::new(SceneCamera::default(), [6, 1, 3]);
        let lighting = ModelLighting {
            direction: [0.0, 1.0, 0.0],
            directional_word: 0x0040_8000,
            ambient: [-8, 32, 64],
            ambient_word: 0,
        };
        let light = SceneModelLight::from_model(lighting);
        assert_eq!(light.color, [0.0, 1.0, 0.5]);
        assert_eq!(light.ambient, [-0.0625, 0.25, 0.5]);
        let other = SceneModelLight {
            ambient: [0.0; 3],
            ..light
        };
        assert_eq!(builder.model_light(light), 0);
        assert_eq!(builder.model_light(other), 1);
        assert_eq!(builder.model_light(light), 0);
        assert_eq!(builder.model_light(other), 1);
        assert_eq!(builder.feed.model_lights.len(), 2);
    }

    /// A pick-up drawn in the same light as another model keeps its share
    /// of it in a light of its own; the models after it keep none.
    #[test]
    fn a_pickup_keeps_its_share_of_retails_light() {
        let mut builder = SceneFeedBuilder::new(SceneCamera::default(), [6, 1, 3]);
        let light = SceneModelLight::from_model(ModelLighting {
            direction: [0.0, 1.0, 0.0],
            directional_word: 0x0040_8000,
            ambient: [-8, 32, 64],
            ambient_word: 0,
        });
        assert_eq!(builder.model_light(light), 0);
        builder.own_light = 0.85;
        assert_eq!(builder.model_light(light), 1);
        assert_eq!(builder.feed.model_lights[1].own, 0.85);
        builder.own_light = 0.0;
        assert_eq!(builder.model_light(light), 0);
        assert_eq!(builder.feed.model_lights[0].own, 0.0);
    }

    /// An additive model face over an opaque face's own corners (a crystal's
    /// shell, a flame layer) is a glow layer, not a rival draw: settling the
    /// faces shrinks neither.
    #[test]
    fn an_additive_glow_layer_keeps_the_opaque_face_under_it() {
        let mut builder = SceneFeedBuilder::new(SceneCamera::default(), [6, 1, 3]);
        let corner = |x: f32, z: f32| SceneVertex {
            position: [x, 0.0, z],
            ..SceneVertex::default()
        };
        let corners = [corner(0.0, 0.0), corner(10.0, 0.0), corner(10.0, 10.0)];
        builder.face(
            &corners,
            Some(1),
            SceneSurface::Opaque,
            Some(3),
            None,
            false,
        );
        builder.submitted(10);
        builder.face(&corners, Some(2), SceneSurface::Glow, Some(3), None, false);
        builder.submitted(5);
        builder.settle_faces();
        for triangle in &builder.feed.triangles {
            let [a, b, c] = triangle.vertices;
            assert!(a != b && b != c, "{:?} shrank", triangle.surface);
        }
    }

    /// A sunk billboard becomes the tracer's glow, which the water adds
    /// below it: two triangles at its command's corners, the waiting slot
    /// taken, and the raster told it was placed.
    #[test]
    fn a_sunk_billboard_becomes_the_tracers_glow() {
        let camera = SceneCamera {
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            focal: [320.0, 240.0],
            centre: [320.0, 240.0],
            ..SceneCamera::default()
        };
        let mut builder = SceneFeedBuilder::new(camera, [6, 1, 3]);
        builder.sunk_billboard = Some((4, 12));
        let corners = [
            ([100, 80], 400),
            ([300, 80], 400),
            ([300, 240], 400),
            ([100, 240], 400),
        ];
        assert!(builder.place_sunk_billboard(corners, [200, 180, 40]));
        assert!(
            builder.sunk_billboard.is_none(),
            "the waiting slot is taken"
        );
        let faces: Vec<_> = builder
            .feed
            .triangles
            .iter()
            .filter(|triangle| triangle.surface == SceneSurface::Glow)
            .collect();
        assert_eq!(faces.len(), 2);
        for face in faces {
            assert_eq!(face.model, Some(12));
            assert_eq!(face.texture, Some(4));
        }
        // The corners are the command's, eye-relative: four of them, each
        // its depth out, and the top pair level with each other.
        let vertices = &builder.feed.vertices;
        assert_eq!(vertices.len(), 4);
        assert!(
            vertices
                .iter()
                .all(|vertex| vertex.color == [200, 180, 40, 255]),
            "in the quad's own colour"
        );
        let length =
            |position: [f32; 3]| position.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
        assert!(
            vertices
                .iter()
                .all(|vertex| length(vertex.position) >= 399.0),
            "every corner stands its depth out"
        );
        assert!(vertices
            .iter()
            .all(|vertex| vertex.position.iter().all(|axis| axis.is_finite())));
        // A corner behind the eye places nothing.
        let mut behind = SceneFeedBuilder::new(camera, [6, 1, 3]);
        behind.sunk_billboard = Some((4, 12));
        assert!(!behind.place_sunk_billboard(
            [
                ([100, 80], -10),
                ([300, 80], 400),
                ([300, 240], 400),
                ([100, 240], 400),
            ],
            [255; 3],
        ));
        assert!(behind.feed.triangles.is_empty());
        // Nothing waiting places nothing.
        let mut empty = SceneFeedBuilder::new(camera, [6, 1, 3]);
        assert!(!empty.place_sunk_billboard(corners, [255; 3]));
        assert!(empty.feed.triangles.is_empty());
    }

    /// Dual fire's two bolts fly 136 apart (guns at -70 and +66), within a
    /// quarter of a plasma bolt's 700 reach: gathered, they would light from
    /// their middle; each bolt keeps a light of its own.
    #[test]
    fn side_by_side_bolts_keep_their_own_lights() {
        let bolt = SceneGlow {
            texture: Some(3),
            strength: 0.5,
            reach: 700.0,
            model: u16::MAX,
            ..Default::default()
        };
        let mut gathered = SceneFeedBuilder::new(SceneCamera::default(), [6, 1, 3]);
        gathered.glow([-70.0, -180.0, 800.0], bolt);
        gathered.glow([66.0, -180.0, 800.0], bolt);
        let glows = &gathered.feed.glows;
        assert_eq!((glows.len(), glows[0].position[0]), (1, -2.0));
        let mut lone = SceneFeedBuilder::new(SceneCamera::default(), [6, 1, 3]);
        lone.lone_glow([-70.0, -180.0, 800.0], bolt);
        lone.lone_glow([66.0, -180.0, 800.0], bolt);
        let positions: Vec<f32> = lone
            .feed
            .glows
            .iter()
            .map(|glow| glow.position[0])
            .collect();
        assert_eq!(positions, [-70.0, 66.0]);
        assert!(lone.feed.glows.iter().all(|glow| glow.faces == 1));
    }
}
