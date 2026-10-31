//! The modern look's tuning: the values the Vulkan renderer's upgrades take,
//! kept out of the code so they can be set by eye
//! ([the plan](../../../docs/INFESTATION-RT-IMPLEMENTATION.md#tuning)). For
//! now the fog, the sky, the water's splashes, the models' materials and
//! thermal vision.
//!
//! The values are built in (`modern_tuning.json`): `worlds` overrides the
//! fog, the sky and the water again for one world (by its number). The game
//! reads no tuning file of its own (2026-10-27: the file is dropped); the
//! tests lay overrides over the built-in values
//! ([`ModernTuning::read`]), objects merging key by key.

use std::{collections::BTreeMap, rc::Rc};

use serde::Deserialize;
use serde_json::Value;

const DEFAULTS: &str = include_str!("modern_tuning.json");

/// The fog: retail's far fade as a haze over the ground, and a mist lit by
/// the sun and retail's lights ([the plan](../../../docs/INFESTATION-RT-IMPLEMENTATION.md#fog)).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FogTuning {
    /// The mist's optical depth across the world's fog distance at its base
    /// height (0: no mist).
    pub density: f32,
    /// The height, in world units, over which the mist thins by e.
    pub height: f32,
    /// The mist's base height above the world's low ground (or its water).
    pub base: f32,
    /// The share of the mist's density it keeps at any height.
    pub floor: f32,
    /// How strongly the mist scatters forward (Henyey-Greenstein g), so it
    /// glows towards the sun.
    pub anisotropy: f32,
    /// The sun's share of the mist's light, the sky's the rest.
    pub sun: f32,
    /// How strongly retail's dynamic lights glow in the mist.
    pub glow: f32,
    /// The share of the mist and the distance fog laid over the sky (1:
    /// all of it, as its rays meet it; 0: none).
    pub sky: f32,
    /// Under thermal vision, which has no mist: how far the ground keeps
    /// its relief, in fog distances (retail's far terrain reaches 1.5),
    /// where the haze is complete, and how quickly it thickens toward there.
    pub reach: f32,
    pub haze: f32,
    /// The distance fog, which the mist's grid carries in a world's view
    /// and which hides what retail draws only out to its fog's end F (the
    /// plan's item 5): where it starts, as a share of retail's fog start;
    /// how much of what lies at F near the ground it hides (0.98: 98%);
    /// and how its thickness grows from its start (1: alike all the way;
    /// above 1, thinner near and thicker far).
    pub start: f32,
    pub opacity: f32,
    pub curve: f32,
    /// Its top, over the world's high ground (the height four fifths of its
    /// land lies below), above which it thins over `band` to its `above`
    /// share, so the upper sky stays readable. Each world takes its own
    /// distance (`worlds`): the land worlds start later, the water worlds
    /// and Altair Prime's smog nearer, the airless worlds latest.
    pub top: f32,
    pub band: f32,
    pub above: f32,
    /// How far below an eye that climbs past the top the top follows it, as
    /// a share of `band` (0: at the eye), so from high up the fog stays at
    /// the eye's height, thinner, and stands lower over the horizon
    /// (2026-10-29).
    pub lag: f32,
    /// The share of F past which a model's pixels dissolve at random, the
    /// more the nearer F, so what rises above the fog fades in rather than
    /// popping (1: none).
    pub dissolve: f32,
    /// Under the tracer, retail's draw distance (its fog's end) times this,
    /// so a world can see further than retail's 12,544 (1: retail's).
    pub far: f32,
    /// Under the tracer, how far past the fog's end things still draw, as a
    /// share of it: they come in inside the whole fog and dissolve in, so
    /// nothing pops where the fog has thinned (1: at it, as retail;
    /// 2026-10-26: take the draw distance a bit past the fog).
    pub past: f32,
    /// The fog's colour, linear RGB, in place of retail's fog and far
    /// colours (the Command Centre's open blue); none keeps retail's.
    pub color: Option<[f32; 3]>,
}

impl FogTuning {
    /// `t` of the way from this fog to `other`.
    pub fn mix(&self, other: &Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            density: mix(self.density, other.density),
            height: mix(self.height, other.height),
            base: mix(self.base, other.base),
            floor: mix(self.floor, other.floor),
            anisotropy: mix(self.anisotropy, other.anisotropy),
            sun: mix(self.sun, other.sun),
            glow: mix(self.glow, other.glow),
            sky: mix(self.sky, other.sky),
            reach: mix(self.reach, other.reach),
            haze: mix(self.haze, other.haze),
            start: mix(self.start, other.start),
            opacity: mix(self.opacity, other.opacity),
            curve: mix(self.curve, other.curve),
            top: mix(self.top, other.top),
            band: mix(self.band, other.band),
            above: mix(self.above, other.above),
            lag: mix(self.lag, other.lag),
            dissolve: mix(self.dissolve, other.dissolve),
            far: mix(self.far, other.far),
            past: mix(self.past, other.past),
            color: match (self.color, other.color) {
                (Some(from), Some(to)) => Some(std::array::from_fn(|i| mix(from[i], to[i]))),
                (from, to) => {
                    if t < 0.5 {
                        from
                    } else {
                        to
                    }
                }
            },
        }
    }
}

/// Thermal vision under Vulkan: each surface's heat, from 0 the coldest to
/// 1 the hottest, and the camera's character
/// ([the plan](../../../docs/INFESTATION-RT-IMPLEMENTATION.md#infrared-vision)).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThermalTuning {
    pub sky: f32,
    pub water: f32,
    pub ground: f32,
    /// The strength of the lamp at the eye: the ACAM's infrared projector
    /// lights what the camera sees by its colour's brightness, with no
    /// shadows, falling off over infrared's reach.
    pub lamp: f32,
    /// How much retail's particles' lights (explosions, fire, hits, muzzle
    /// flashes) and the shots' warm what they light, by their own colour
    /// before infrared's green; lamps warm nothing.
    pub lights: f32,
    /// The half-life in seconds of the heat those lights leave on the
    /// ground (the heat memory: scorch marks, warm spots where shots hit).
    pub memory: f32,
    /// How much warmer a moving object runs (engines, running creatures),
    /// reached at `[1]` world units a second.
    pub moving: (f32, f32),
    /// How much a new shot warms the guns that fired it, and the half-life
    /// in seconds they cool by.
    pub fired: (f32, f32),
    /// The names of the models that are guns (`*` as in `names`): their
    /// barrels and heads, not the towers, struts and mounts they stand on.
    pub guns: Vec<String>,
    /// The names of wrecks and corpses: one whose object was seen alive
    /// keeps the heat it had over its own, losing half every `cooling`
    /// seconds.
    pub wrecks: Vec<String>,
    pub cooling: f32,
    /// Surfaces retail draws unlit: glows and lamps.
    pub glowing: f32,
    /// A model no name below matches.
    pub models: f32,
    /// Model names and their heat, the first match winning; `*` stands for
    /// any run of characters.
    pub names: Vec<(String, f32)>,
    /// The share of the way the camera's range follows each frame's.
    pub rate: f32,
    /// The shares of the scene's heat (the sky left out) the camera leaves
    /// below black and above white: a few hot pixels no longer wash out
    /// the rest.
    pub gain: [f32; 2],
    /// The camera's width in pixels.
    pub resolution: f32,
    /// How much of the image's fine contrast the camera adds back, so a
    /// narrow gain keeps relief and figures (detail enhancement).
    pub detail: f32,
    /// The grain each frame, the sensor's fixed stripes, edge sharpening,
    /// the halo around hot things and the vignette, as strengths.
    pub grain: f32,
    pub stripes: f32,
    pub sharpen: f32,
    pub halo: f32,
    pub vignette: f32,
}

impl ThermalTuning {
    /// The heat of the model named `name`.
    pub fn heat_of(&self, name: &str) -> f32 {
        self.listed(name).unwrap_or(self.models)
    }

    /// The heat `names` gives the model named `name`, if any.
    pub fn listed(&self, name: &str) -> Option<f32> {
        self.names
            .iter()
            .find(|(pattern, _)| glob(pattern.as_bytes(), name.as_bytes()))
            .map(|(_, heat)| *heat)
    }

    /// Whether the model named `name` is a gun.
    pub fn is_gun(&self, name: &str) -> bool {
        any_glob(&self.guns, name)
    }

    /// Whether the model named `name` is a wreck or a corpse.
    pub fn is_wreck(&self, name: &str) -> bool {
        any_glob(&self.wrecks, name)
    }
}

/// [`WaterTuning::sunk`]'s default: a pick-up sunk under the water shows
/// about two thirds of what retail's blend adds (2026-10-25: at
/// all of it, too visible; trimmed to 60-70%).
fn default_sunk_pickups() -> f32 {
    0.65
}

/// Whether `name` matches one of `patterns`.
fn any_glob(patterns: &[String], name: &str) -> bool {
    patterns
        .iter()
        .any(|pattern| glob(pattern.as_bytes(), name.as_bytes()))
}

/// Whether `name` matches `pattern`, where `*` stands for any run of
/// characters.
fn glob(pattern: &[u8], name: &[u8]) -> bool {
    let [mut p, mut n] = [0, 0];
    // The last star seen and where its run now ends in `name`.
    let mut star = None;
    while n < name.len() {
        if pattern.get(p) == Some(&b'*') {
            star = Some((p, n));
            p += 1;
        } else if pattern.get(p) == Some(&name[n]) {
            p += 1;
            n += 1;
        } else if let Some((at, end)) = star {
            p = at + 1;
            n = end + 1;
            star = Some((at, end + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
}

/// The vehicles' headlamps under the tracer:
/// two lamps on the nose throwing a shaped beam in place of retail's grey
/// pool.
/// - `color`, and `intensity`: the light on a surface square to the beam
///   1,000 units out, where 1 is full light (a shade of 128), in linear
///   light, where it falls off with the square of the distance (the rear
///   lamps' as a display tone, decoded as retail's lights are); what glows
///   in the beam (the air, the water, the crystals) takes it as a display
///   tone;
/// - `position`: a lamp in model units, mirrored across the nose for the
///   other;
/// - `pitch`: how far below the nose the beam points, in degrees;
/// - `spread`: the beam's half-angle across, and `inner` the share of it
///   at full strength;
/// - `cutoff`: its top edge above the axis, fading over `soft` degrees;
/// - `hot`: the angle below the axis over which it thins, as a low beam
///   does, so the ground just ahead (near, and so lit the most) does not
///   outshine the ground 1,000 to 2,000 units out;
/// - `near`: the distance its falloff eases near the lamp, and `reach`
///   where it ends;
/// - `wrap`: how far the ground's facing wraps, so the ground it grazes
///   still takes it;
/// - `water_scatter`: how much of its light entering the water glows back in
///   the water's own colour, and `water_gloss` how rough the ripples its
///   glints show in are;
/// - `air`: how much it glows in the air, through the world's mist or, where
///   that is thinner, `air_floor` (per thousand units) of its own, scattered
///   as the mist scatters the sun (mostly onward, so faintest from behind);
/// - `lens`: the radius of each lamp's lens on the nose, and `glare` how
///   strongly a lamp facing the eye glares (0: none);
/// - `water_glow`: how strongly a lamp under the water lights the water
///   itself, scattering back in its colour as its clarity absorbs it;
/// - `rear`: the rear lamps ([`RearLampTuning`]).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeadlightTuning {
    pub color: [f32; 3],
    pub intensity: f32,
    pub position: [f32; 3],
    pub pitch: f32,
    pub spread: f32,
    pub inner: f32,
    pub cutoff: f32,
    pub soft: f32,
    pub hot: f32,
    pub near: f32,
    pub reach: f32,
    pub wrap: f32,
    pub water_scatter: f32,
    pub water_gloss: f32,
    pub air: f32,
    pub air_floor: f32,
    pub lens: f32,
    pub glare: f32,
    pub water_glow: f32,
    pub rear: RearLampTuning,
}

/// The buggy's and truck's rear lamps as lights under the tracer: retail's red pair while it stands or coasts, its white
/// pair reversing, as its rear glow is lit. `red` and `white` are the pairs'
/// colours, `pitch` how far below straight back they point (they sit under
/// the sloping back, facing down and back), `size` how far across they cast
/// their shadows from (the rear pods beside them would cut a point's light
/// into hard edges), and the rest their beam, as the headlamps'.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RearLampTuning {
    pub red: [f32; 3],
    pub white: [f32; 3],
    pub intensity: f32,
    pub pitch: f32,
    pub spread: f32,
    pub inner: f32,
    pub cutoff: f32,
    pub soft: f32,
    pub hot: f32,
    pub near: f32,
    pub reach: f32,
    pub size: f32,
}

/// A lamp's beam as `pt_frame.glsl`'s `LampLook` takes it: clamped, its
/// angles in radians; the headlamps' `size` is 0, a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LampLook {
    pub intensity: f32,
    pub spread: f32,
    pub inner: f32,
    pub cutoff: f32,
    pub soft: f32,
    pub hot: f32,
    pub near: f32,
    pub reach: f32,
    pub size: f32,
}

impl HeadlightTuning {
    /// The headlamps' beam, or with `rear` the rear lamps'.
    pub fn look(&self, rear: bool) -> LampLook {
        let r = &self.rear;
        let [intensity, spread, inner, cutoff, soft, hot, near, reach, size] = if rear {
            [
                r.intensity,
                r.spread,
                r.inner,
                r.cutoff,
                r.soft,
                r.hot,
                r.near,
                r.reach,
                r.size,
            ]
        } else {
            [
                self.intensity,
                self.spread,
                self.inner,
                self.cutoff,
                self.soft,
                self.hot,
                self.near,
                self.reach,
                0.0,
            ]
        };
        LampLook {
            intensity: intensity.max(0.0),
            spread: spread.clamp(1.0, 89.0).to_radians(),
            inner: inner.clamp(0.0, 1.0),
            cutoff: cutoff.clamp(-45.0, 45.0).to_radians(),
            soft: soft.clamp(0.1, 45.0).to_radians(),
            hot: hot.clamp(0.1, 45.0).to_radians(),
            near: near.max(1.0),
            reach: reach.max(1.0),
            size: size.max(0.0),
        }
    }
}

/// The scenery crystals catching the light under the tracer (step 5): their models' names (`*` as in the thermal
/// names); how much of the light reaching one it glows back in its colour,
/// how strongly the facets turned to a lamp flash its white, and how far
/// the light it then throws reaches; and each world's colour, by world
/// number (others take the world's own light).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CrystalTuning {
    pub names: Vec<String>,
    pub glow: f32,
    pub flash: f32,
    pub reach: f32,
    pub colors: BTreeMap<u32, [f32; 3]>,
    #[serde(default)]
    pub flare: Option<CrystalFlareTuning>,
}

/// A crystal's flare where a gun's bolt strikes it: the colour it turns (`color` for a plasma bolt, `laser_color`
/// for a laser beam), the glow it gains, the share of that glow it throws
/// on what is round it (`under`), and how far its light then reaches at the
/// strike, fading out over `seconds`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CrystalFlareTuning {
    pub color: [f32; 3],
    #[serde(default = "laser_yellow")]
    pub laser_color: [f32; 3],
    pub strength: f32,
    #[serde(default = "whole")]
    pub under: f32,
    pub reach: f32,
    pub seconds: f32,
}

fn laser_yellow() -> [f32; 3] {
    [1.0, 0.8, 0.15]
}

fn whole() -> f32 {
    1.0
}

impl CrystalFlareTuning {
    /// The colour a crystal turns where `kind` struck it.
    pub fn color_of(&self, kind: crate::crystal_flares::StrikeKind) -> [f32; 3] {
        match kind {
            crate::crystal_flares::StrikeKind::Plasma => self.color,
            crate::crystal_flares::StrikeKind::Laser => self.laser_color,
        }
    }
}

impl CrystalTuning {
    /// Whether the model named `name` is one of the crystals.
    pub fn is_crystal(&self, name: &str) -> bool {
        any_glob(&self.names, name)
    }
}

/// The round parts under the tracer ([`crate::round_parts`]): their faces split onto the true circle round
/// an axis of their own model, so finely that no piece strays more than
/// `tolerance` pixels from it (each rim edge in at most `most` pieces).
/// `parts` name the models (`*` as in the thermal names) and their axis;
/// `treads`, their lugs.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RoundTuning {
    pub tolerance: f32,
    pub most: u32,
    pub parts: Vec<RoundPart>,
    pub treads: TreadTuning,
}

/// A round part's treads as real lugs (step
/// 3): the faces in `sprites` are the treads, painted with `lugs` lugs
/// across each sprite; on a wheel more than `from` pixels in radius on
/// screen, every rim edge on a ring they use is cut `per_lug` times a lug
/// and its grooves sunk `depth` units, by the sprite's own brightness (its
/// bright lug tops stay at retail's radius). `depth` 0 turns them off.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TreadTuning {
    pub sprites: Vec<i16>,
    pub lugs: u32,
    pub per_lug: u32,
    pub depth: f32,
    pub from: f32,
}

/// A round part: its models' names, the model axis it turns about, a point
/// on that axis (model units; the origin by default), and for a part inside
/// a larger model (the settled skimmer's jet) the distances from the axis
/// and the heights along it its rims stand within.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RoundPart {
    pub names: Vec<String>,
    pub axis: RoundAxis,
    #[serde(default)]
    pub centre: [f32; 3],
    #[serde(default)]
    pub radius: Option<[f32; 2]>,
    #[serde(default)]
    pub along: Option<[f32; 2]>,
}

/// A model's own axis a round part turns about, or `Sphere`, every axis: a
/// sphere round the part's centre.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RoundAxis {
    X,
    Y,
    Z,
    Sphere,
}

impl RoundTuning {
    /// The round part the model named `name` is, if any.
    pub fn part(&self, name: &str) -> Option<&RoundPart> {
        self.parts.iter().find(|part| any_glob(&part.names, name))
    }
}

/// A lighting preset ([`crate::modern_settings::Lighting`]) under the
/// tracer: the frame's grade before the HUD, and the strength of what
/// catches the eye. Remaster's keep the look as tuned: 1, and 0 for `curve`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LightingTuning {
    /// The frame's brightness, multiplied.
    pub exposure: f32,
    /// Its colours' distance from grey, multiplied.
    pub saturation: f32,
    /// How far its tones bend toward an S-curve, darks darker and lights
    /// brighter (0: not at all).
    pub curve: f32,
    /// Metal's and the water's reflections, multiplied: a metal mirrors
    /// more and more sharply.
    pub reflections: f32,
    /// The sun's share of the mist's light, its shafts, multiplied.
    pub shafts: f32,
    /// The glowing models' and retail's lights' strength, multiplied.
    pub glows: f32,
    /// How much of the blended effects (bolts, flames, fireballs) the water
    /// and metal mirror, on top of their lights' glints (0: none).
    pub effects: f32,
    /// The thrusters' trails' density, multiplied (0: none).
    pub trails: f32,
}

impl LightingTuning {
    /// Whether the frame's colours change, so it needs the grade.
    pub fn grades(&self) -> bool {
        self.exposure != 1.0 || self.saturation != 1.0 || self.curve != 0.0
    }
}

/// The two lighting presets' numbers.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LightingPresets {
    pub remaster: LightingTuning,
    pub eye_candy: LightingTuning,
}

/// Which sky a world shows under the tracer.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SkyStyle {
    /// Retail's painting, which the raster draws.
    Painted,
    /// The renderer's own: a gradient, the sun, stars and volumetric clouds.
    Modern,
}

/// A cloud layer's shape: how its density runs from its base to its top.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CloudShape {
    /// Round tops on flat bases.
    Cumulus,
    /// Thin sheets.
    Stratus,
    /// Tall towers spreading at the top.
    Storm,
    /// Low and thick, dense from the base.
    Ash,
}

/// The sun in the sky, which stands where the world's light comes from.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SunTuning {
    /// Its disc's angular radius in degrees, in the modern sky.
    pub radius: f32,
    /// The angular radius its shadows are cast from, in degrees, under
    /// either sky: how soft they grow with distance from what casts them
    /// (0: hard). Wider than the drawn disc: a degree's shadows stayed
    /// hard and very dark (the plan's item 17).
    pub shadow: f32,
    /// The glow round it and its disc, as strengths, in the modern sky.
    pub glow: f32,
    pub disc: f32,
}

/// The clouds: volumetric all over the modern sky; under retail's painting
/// only their shadows, drifting over the ground.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CloudTuning {
    pub shape: CloudShape,
    /// How much of the sky they cover (0: none).
    pub cover: f32,
    /// The layer's bottom and top, as world heights.
    pub base: f32,
    pub top: f32,
    /// How wide the largest clouds are, in world units, and how fast they
    /// drift, in world units a clock tick.
    pub size: f32,
    pub speed: f32,
    /// Their sunlit and shaded colours, 0 to 1; none takes the world's
    /// light for the lit side and the zenith's colour, darker, for the
    /// shaded one.
    pub lit: Option<[f32; 3]>,
    pub shade: Option<[f32; 3]>,
    /// How much of the sun their shadows take from the ground (0 to 1).
    pub shadow: f32,
    /// The colour of veins glowing inside them, slowly flickering
    /// (Tabuchi's storm); none has none.
    pub veins: Option<[f32; 3]>,
    /// The colour of light from below on their undersides (lava, Artoria's
    /// clarifiers); none has none.
    pub under: Option<[f32; 3]>,
    /// How many times longer than wide they stretch across X, torn by the
    /// wind (1: not at all).
    pub streak: f32,
    /// How far off they fade into the horizon's haze, by e every this many
    /// world units: long for dense smog that stays dark to the horizon.
    pub haze: f32,
    /// How much of the light inside them scatters on, 0 to 1: 1 for water
    /// clouds, bright through their depth; low for smog, which absorbs it,
    /// so its bodies stay dark and only its edges catch the sun.
    pub albedo: f32,
    /// How far they round off toward the layer's top, 0 to 1: 1 domes that
    /// thin as they rise; 0 full-bodied clouds that keep their footprint up
    /// to their crowns (picked on Command Centre, 2026-10-23).
    pub round: f32,
    /// How far each cloud's base and top stand above or below the layer's,
    /// at most, as a share of its thickness, smoothly across the sky (0:
    /// one flat sheet).
    pub rise: f32,
}

/// Flashes inside the modern sky's clouds, each briefly lighting the world
/// (never a strobe: at most one every few seconds).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LightningTuning {
    /// About how many flashes a second (0: none).
    pub rate: f32,
    pub color: [f32; 3],
    /// How much a flash at its peak brightens the world.
    pub light: f32,
}

/// The sky under the tracer, world by world
/// ([the plan](../../../docs/rt-plans/sky-per-world.md)): retail's painting
/// until a world's recipe is approved, then the renderer's own.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SkyTuning {
    pub style: SkyStyle,
    /// The colour overhead and at the horizon, 0 to 1; none takes retail's:
    /// its clear colour (H7 `0x72`) overhead, its far terrain's colour
    /// (`0x73`) at the horizon, where the haze meets the sky.
    pub zenith: Option<[f32; 3]>,
    pub horizon: Option<[f32; 3]>,
    /// How soon the horizon's colour gives way to the zenith's, rising.
    pub curve: f32,
    /// A warm glow low on the horizon toward the sun, for the sunset
    /// worlds (0: none).
    pub horizon_glow: f32,
    pub sun: SunTuning,
    /// How many stars show (0: none).
    pub stars: f32,
    pub clouds: CloudTuning,
    pub band: BandTuning,
    /// The first and last H3 sprites of retail's additive sky art to keep
    /// under the modern sky (the star domes, the Moon, the glows): added to
    /// it where a ray meets nothing, as retail's blend added them. None
    /// keeps none.
    pub objects: Option<[i16; 2]>,
    pub precip: PrecipTuning,
    pub lightning: LightningTuning,
    /// How strongly the modern sky lights what it shines on under full
    /// path tracing, against its drawn colour: retail's ambient (the tint
    /// over four) stood for all the light from the sky and from bounces,
    /// so the sky's light is tuned apart from its look
    /// ([the plan](../../../docs/rt-plans/full-path-tracing.md)).
    pub light: f32,
    /// A world hanging in the sky (Foldrak Noir's "enormous artificial
    /// alien homeworld"): none keeps the sky empty.
    #[serde(default)]
    pub planet: Option<PlanetTuning>,
}

/// A world in the modern sky, drawn by the sky itself (the final
/// stretch's E6): where it hangs (degrees round from +Z toward +X, and
/// up from the horizon), how wide (its angular radius in degrees), its
/// colour lit by the sun, and how bright the lights on its night side are.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PlanetTuning {
    pub direction: [f32; 2],
    pub size: f32,
    pub color: [f32; 3],
    pub lights: f32,
}

/// What falls from the modern sky (`pt_weather.glsl`): snow or rain, drawn
/// over the world at the display's size.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PrecipTuning {
    pub kind: PrecipKind,
    /// The share of the field's cells that hold a flake or a drop, 0 to 1.
    pub density: f32,
    /// How fast it falls, and drifts across X and Z, in world units a
    /// second.
    pub fall: f32,
    pub wind: [f32; 2],
    /// A flake's or a drop's size, and a drop's streak, in world units.
    pub size: f32,
    pub length: f32,
    /// Its colour, lit by the world's light.
    pub color: [f32; 3],
    /// It falls only as night falls, as thick as night has fallen
    /// ([`crate::world_scene::nightfall`]), and over a painted sky too
    /// (Pelagia's rain): otherwise only the modern sky has weather.
    #[serde(default)]
    pub with_night: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PrecipKind {
    None,
    Snow,
    Rain,
    /// Grey flakes drifting, lit as snow is (Sanrel-Dek, Shak'tahh).
    Ash,
    /// Sparks glowing in their own colour, each flickering, rising where
    /// `fall` is negative.
    Embers,
}

/// The painting's land kept under the modern sky: the backdrop's tiles
/// that hold it, traced as far ground with the painted sky above each
/// column's skyline cut away (`d3d_texture::cut_band`). The rest of the
/// backdrop, and what hangs on it, is the modern sky's to replace.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BandTuning {
    /// The first and last H3 sprites of the land's tiles; none keeps no
    /// band.
    pub sprites: Option<[i16; 2]>,
    /// How far, 0 to 1, a texel's colour may lie from the painted sky's
    /// and still be sky.
    pub cut: f32,
    /// A texel at least this bright (0 to 1, in luminance) is sky too: a
    /// glow painted in the sky (Exxodus's) is far from the sky's colour but
    /// brighter than the land. None: colour alone decides.
    pub bright: Option<f32>,
    /// Where each column's skyline is looked for: up from the bottom, to the
    /// first sky (clouds painted above the land go), or down from the top,
    /// to the first land (for land whose texture strays near the sky's
    /// colour, as Caspadril's snow does).
    pub from: BandSearch,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BandSearch {
    #[default]
    Bottom,
    Top,
}

/// The traced water: how clear and how reflective it is, retail's splashes
/// on it and the blasts in it
/// ([the plan](../../../docs/INFESTATION-RT-IMPLEMENTATION.md#water)).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WaterTuning {
    /// How far one sees into the water before its own colour hides the
    /// bottom, in world units (some levels have clearer water and some
    /// dirtier): along the refracted ray the bottom fades to 1/e
    /// at one and a half of it and is gone by about three
    /// (pt_trace.comp's `water_clear`); a sunk pick-up's glow fades by e
    /// every five of it in depth (`sunk_glow`).
    pub clarity: f32,
    /// How strongly a pick-up sunk under the water shows through it, 0 to
    /// 1 and past (1: as retail's blend adds it; 0.65 by default). The
    /// water over one is shallow, so its depth fade leaves most of its
    /// light: this trims how bright it reads (2026-10-25: a bit too visible,
    /// then trimmed to 60-70%).
    #[serde(default = "default_sunk_pickups")]
    pub sunk: f32,
    /// The least share of its surroundings the surface mirrors, looking
    /// straight down (water's own is 0.02); more makes murky water shine
    /// (water should have some reflectivity, especially on
    /// levels with dirty water).
    pub reflect: f32,
    /// How brightly lava glows, for the worlds whose water tile is lava (0:
    /// water). Lava neither mirrors nor shows what lies under it.
    #[serde(default)]
    pub glow: f32,
    /// How steeply a splash's ring tilts the surface (0: no rings).
    pub rings: f32,
    /// How white a young splash's foam is (0: none).
    pub foam: f32,
    /// How far a ring spreads over the splash's life, and how wide it is,
    /// in world units at a splash's own scale.
    pub spread: f32,
    pub width: f32,
    /// A blast in the water (`scene_spray`): how high the plume of one just
    /// under the surface rises, in world units (0: none), the share of its
    /// drops thrown (0 to 1), and how brightly lights under the water (its
    /// fireballs') glow up through it (0: none).
    pub plume: f32,
    pub drops: f32,
    pub flash: f32,
    /// The shore (the plan's item 2): over how much depth, in world units,
    /// the water fades in from nothing at its edge (its reflection, ripples
    /// and glints), how far that edge wanders, in units of depth, and how
    /// strongly a thin foam line marks it, creeping slowly up and back as
    /// small waves lap (0: none).
    pub shore: f32,
    pub wobble: f32,
    pub lap: f32,
    /// How high above the water the ground looks damp, in world units (0:
    /// none: the snow, ice and lava worlds), and how much darker damp
    /// ground, and the ground under the water, is (1: no darker).
    pub wet: f32,
    pub damp: f32,
    /// The spray a blast throws up (the plan's item 3): the share of what
    /// lies behind a clear drop it shows (white water shows less), and how
    /// much of its liquid's colour it takes.
    pub see_through: f32,
    pub tint: f32,
    /// How much lighter the water a blast churned stays for its first
    /// seconds (0: none).
    pub churn: f32,
}

/// The bounce light: the light path tracing adds off what is near, which
/// retail never drew.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BounceTuning {
    /// How much of its colour the bounce light keeps, 0 to 1, its
    /// brightness kept: at 1 the sunlit grass cast its green on the stores'
    /// shaded sides (chosen 2026-10-23: less colour, the same
    /// light).
    pub colour: f32,
}

/// The models' materials under the tracer
/// ([the plan](../../../docs/INFESTATION-RT-IMPLEMENTATION.md#materials)): how
/// metallic each is, from 0 (matte, as all were) to 1, and how rough its
/// reflections are, from 0 (a mirror) to 1.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MaterialTuning {
    /// Model names and their metalness and roughness, the first match
    /// winning; `*` stands for any run of characters. Others are matte.
    pub names: Vec<(String, f32, f32)>,
    /// Sprites (H3 slots) whose faces take a material of their own over
    /// their model's: `[sprite, metal, rough]`, or `[sprite, metal, rough,
    /// [v, metal, rough]]`, the sprite's rows above `v` (a share of its
    /// height from the top) taking the second. The ACAM's tyres are rubber
    /// and their hubs metal: only the wheels
    /// draw sprites 195 to 198, and the truck's hub (198) is the tyre's side
    /// wall (196) in its top half over the buggy's hub (197): rows 0 to 29
    /// of 60 are 196's, 0 to 14 of 30 at low detail (its row 14 up to 8
    /// levels off 196's, still side wall). A sprite's material is the bare
    /// surface, no paint: the rain darkens and smooths its rubber rather
    /// than lay the paint's clear coat over it (a milky sheen over the
    /// black), and wets its metal as all metal.
    #[serde(default)]
    pub sprites: Vec<SpriteMaterial>,
    /// Model names that glow, each with its light's strength (its sprite's
    /// colour times it, or a colour of its own) and its reach in world
    /// units: the crystals, the active warpgate, the power transformer.
    #[serde(default)]
    pub glows: Vec<GlowEntry>,
    /// How much of the light retail gives the pick-ups the tracer keeps, 0
    /// to 1, the rest traced: retail lights each in its own pulsing colour,
    /// unshadowed, so they read in any light (2026-10-23: far less affected
    /// by light, being pick-ups).
    #[serde(default)]
    pub pickups: f32,
    /// The guns' shots in flight, which retail lets light nothing: the
    /// bolts (the plasma's among them) and the laser beams. The strength of
    /// each one's light (its sprite's colour times it) and its reach in
    /// world units (0: no light).
    #[serde(default)]
    pub shots: (f32, f32),
    /// The cannon's sparks, which retail lets light nothing either: each
    /// one's light and reach as for the shots (2026-10-23: not
    /// as much as plasmas, but a little).
    #[serde(default)]
    pub sparks: (f32, f32),
    /// Model names whose blended edges, while drawn, span a field of energy
    /// between them (`scene_field`: the active warpgate's lightning), each
    /// with how strongly it shows and its colour.
    #[serde(default)]
    pub fields: Vec<(String, f32, [f32; 3])>,
    /// Model names whose blended edges and flares the trace's frame
    /// redraws as crisp lightning ([`crate::scene_gate`]: the active
    /// warpgate's arcs), each with its look.
    #[serde(default)]
    pub gates: Vec<GateTuning>,
    /// Model names whose faces retail drew unlit glow brighter than it drew
    /// them under the tracer (lights, signs, the logo), each with how many
    /// times (`*` as in `names`; 1: as retail). They light what is near them
    /// as brightly, and show past full light (the tonemap pales them). Which
    /// ones is a choice (the full path tracing plan's stage 5): none by
    /// default, every emitter as retail drew it.
    #[serde(default)]
    pub emitters: Vec<(String, f32)>,
    /// Model names whose faces in `sprites` glow with their vehicle's
    /// engine load ([`crate::object_runtime::ObjectArena::engine_loads`]),
    /// the throttle that bites: up to `gain` times their texture at `load`
    /// or more, none at none, easing over `seconds`. The skimmer's thruster
    /// lights (2026-10-31: the user's wish; retail drew them lit, glowing
    /// only through the morph); in the air, where its thrust finds less
    /// grip, they dim as its engine's load drops.
    #[serde(default)]
    pub thrust_glows: Vec<ThrustGlow>,
}

/// A model's faces that glow with its vehicle's thrust
/// ([`MaterialTuning::thrust_glows`]).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThrustGlow {
    pub name: String,
    pub sprites: Vec<i16>,
    pub gain: f32,
    pub seconds: f32,
    pub load: f32,
}

/// A gate's redrawn lightning: its model's name (`*` as in `names`), the
/// bolts' core and halo half-widths as shares of retail's edges', how far a
/// bolt strays from its line (its first step, a share of its length), how
/// bright its colours add, the glints' size as a share of retail's flares',
/// and the steady light at the opening's middle while the spokes show, its
/// strength and reach (0: none).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GateTuning {
    pub name: String,
    pub core: f32,
    pub halo: f32,
    pub jag: f32,
    pub brightness: f32,
    pub glint: f32,
    pub light: (f32, f32),
}

/// A model that glows: `[name, strength, reach]`, its light its sprite's
/// colour times the strength, or `[name, strength, reach, [r, g, b]]` in
/// that colour (0 to 1) instead, or `{"name", "strength", "reach",
/// "sprites"}`, its light only from its parts in those sprites, and with
/// `"gather"` its parts that far apart (world units) give lights of their
/// own (the VTOL's two jets).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum GlowEntry {
    Tinted(String, f32, f32, [f32; 3]),
    Sprite(String, f32, f32),
    Sprites {
        name: String,
        strength: f32,
        reach: f32,
        sprites: Vec<i16>,
        #[serde(default)]
        gather: Option<f32>,
    },
}

/// The light a glowing model gives: its strength, its reach (world units)
/// and its colour, if not its sprite's; and how far apart its parts give
/// lights of their own, if not a quarter of its reach.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    pub strength: f32,
    pub reach: f32,
    pub color: Option<[f32; 3]>,
    pub gather: Option<f32>,
}

/// A sprite's own material ([`MaterialTuning::sprites`]).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SpriteMaterial {
    Whole(i16, f32, f32),
    Split(i16, f32, f32, (f32, f32, f32)),
}

/// A model's material.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Material {
    pub metal: f32,
    pub rough: f32,
}

impl Material {
    fn clamped(metal: f32, rough: f32) -> Self {
        Self {
            metal: metal.clamp(0.0, 1.0),
            rough: rough.clamp(0.02, 1.0),
        }
    }
}

impl MaterialTuning {
    /// Whether any faces of the model named `name` glow with its thrust.
    pub fn glows_with_thrust(&self, name: &str) -> bool {
        self.thrust_glows
            .iter()
            .any(|glow| glob(glow.name.as_bytes(), name.as_bytes()))
    }

    /// How the faces of the model named `name` in sprite `sprite` glow with
    /// its vehicle's thrust, if they do.
    pub fn thrust_glow_of(&self, name: &str, sprite: i16) -> Option<&ThrustGlow> {
        self.thrust_glows.iter().find(|glow| {
            glob(glow.name.as_bytes(), name.as_bytes()) && glow.sprites.contains(&sprite)
        })
    }

    /// The light the model named `name` gives from its parts in sprite
    /// `sprite`, if they glow.
    pub fn glow_of(&self, name: &str, sprite: i16) -> Option<Glow> {
        self.glows.iter().find_map(|entry| {
            let (pattern, strength, reach, color, sprites, gather) = match entry {
                GlowEntry::Tinted(pattern, strength, reach, color) => (
                    pattern,
                    *strength,
                    *reach,
                    Some(color.map(|c| c.clamp(0.0, 1.0))),
                    None,
                    None,
                ),
                GlowEntry::Sprite(pattern, strength, reach) => {
                    (pattern, *strength, *reach, None, None, None)
                }
                GlowEntry::Sprites {
                    name,
                    strength,
                    reach,
                    sprites,
                    gather,
                } => (name, *strength, *reach, None, Some(sprites), *gather),
            };
            (glob(pattern.as_bytes(), name.as_bytes())
                && sprites.is_none_or(|sprites| sprites.contains(&sprite)))
            .then_some(Glow {
                strength: strength.max(0.0),
                reach: reach.max(1.0),
                color,
                gather: gather.map(|apart| apart.max(0.0)),
            })
        })
    }

    /// The field the model named `name` spans with its blended edges, as
    /// its strength and colour, if any.
    pub fn field_of(&self, name: &str) -> Option<(f32, [f32; 3])> {
        self.fields
            .iter()
            .find(|(pattern, _, _)| glob(pattern.as_bytes(), name.as_bytes()))
            .map(|(_, strength, color)| {
                (strength.clamp(0.0, 1.0), color.map(|c| c.clamp(0.0, 1.0)))
            })
    }

    /// The redrawn lightning of the model named `name`, if any, with its
    /// index in `gates`.
    pub fn gate_of(&self, name: &str) -> Option<(usize, &GateTuning)> {
        self.gates
            .iter()
            .enumerate()
            .find(|(_, gate)| glob(gate.name.as_bytes(), name.as_bytes()))
    }

    /// How many times brighter than retail the unlit faces of the model
    /// named `name` glow (1: as retail drew them).
    pub fn emitter_of(&self, name: &str) -> f32 {
        self.emitters
            .iter()
            .find(|(pattern, _)| glob(pattern.as_bytes(), name.as_bytes()))
            .map_or(1.0, |(_, gain)| gain.max(0.0))
    }

    /// The material of the model named `name`.
    pub fn of(&self, name: &str) -> Material {
        self.names
            .iter()
            .find(|(pattern, _, _)| glob(pattern.as_bytes(), name.as_bytes()))
            .map_or(
                Material {
                    metal: 0.0,
                    rough: 1.0,
                },
                |(_, metal, rough)| Material::clamped(*metal, *rough),
            )
    }

    /// The material of the faces in sprite `sprite`, if it has its own,
    /// and the rows above a share of its height with theirs.
    pub fn of_sprite(&self, sprite: i16) -> Option<(Material, Option<(f32, Material)>)> {
        self.sprites.iter().find_map(|entry| match *entry {
            SpriteMaterial::Whole(id, metal, rough) => {
                (id == sprite).then(|| (Material::clamped(metal, rough), None))
            }
            SpriteMaterial::Split(id, metal, rough, (v, above_metal, above_rough)) => {
                (id == sprite).then(|| {
                    (
                        Material::clamped(metal, rough),
                        Some((
                            v.clamp(0.0, 1.0),
                            Material::clamped(above_metal, above_rough),
                        )),
                    )
                })
            }
        })
    }
}

/// The fog's `far` and `past` ([`FogTuning`]) by default (`all`) and for
/// each world whose own differ.
#[derive(Clone, Debug, PartialEq)]
pub struct TracedFar {
    pub all: [f32; 2],
    pub worlds: BTreeMap<u32, [f32; 2]>,
}

impl TracedFar {
    /// World `world`'s `[far, past]`.
    pub fn of(&self, world: u32) -> [f32; 2] {
        self.worlds.get(&world).copied().unwrap_or(self.all)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModernTuning {
    bounce: BounceTuning,
    fog: FogTuning,
    world_fog: BTreeMap<u32, FogTuning>,
    /// The fog a darkened world turns to (`worlds.N.fog_dark`, over its own).
    world_fog_dark: BTreeMap<u32, FogTuning>,
    sky: SkyTuning,
    world_sky: BTreeMap<u32, SkyTuning>,
    water: WaterTuning,
    world_water: BTreeMap<u32, WaterTuning>,
    materials: MaterialTuning,
    thermal: ThermalTuning,
    headlight: HeadlightTuning,
    crystals: CrystalTuning,
    round: RoundTuning,
    ultra: UltraTuning,
    trails: TrailTuning,
    atmosphere: AtmosphereTuning,
    lighting: LightingPresets,
}

/// Ultra ground detail ([`crate::terrain_runtime::ultra`]): the ground keeps a crease where it
/// turns by more than `crease` degrees, and rounds off elsewhere. Under a
/// building it rises to the building's base, by at most `fill`, easing
/// back to its slope over `ease` past it: the terrain's furniture that
/// stands on its base, and the placed models `footings` names (`*` as in
/// the thermal names).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UltraTuning {
    pub crease: f32,
    pub fill: f32,
    pub ease: f32,
    pub footings: Vec<String>,
}

impl UltraTuning {
    pub fn shape(&self) -> crate::terrain_runtime::ultra::UltraShape {
        crate::terrain_runtime::ultra::UltraShape {
            crease: self.crease,
            fill: self.fill,
            ease: self.ease,
        }
    }

    /// Whether the placed model named `name` stands on a footing.
    pub fn footing(&self, name: &str) -> bool {
        any_glob(&self.footings, name)
    }
}

/// The port's ambience ([`crate::ambience`]):
/// each world's bed, a kind of place, and the recordings it takes turns
/// with (names in `Remastered.pak`), changing every `change` seconds (a
/// span) with a crossfade of `crossfade` seconds; a sweetener every `every`
/// seconds (a span); `level` its gain. `sources` are the sounds placed where
/// things stand, in any world; `shores` the sound of each world's plane's
/// edge (water, the Brain's liquid, lava).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AtmosphereTuning {
    pub beds: BTreeMap<String, BedTuning>,
    pub worlds: BTreeMap<u32, String>,
    pub change: [f32; 2],
    pub crossfade: f32,
    pub every: [f32; 2],
    pub level: f32,
    #[serde(default)]
    pub sources: Vec<SourceTuning>,
    #[serde(default)]
    pub shores: BTreeMap<u32, PlacedTuning>,
    #[serde(default)]
    pub reverb: Option<ReverbTuning>,
    /// The crystals' ring where a bolt strikes one (item 26).
    #[serde(default)]
    pub shields: Option<PlacedTuning>,
    /// The weather's sound: wind, rain and the rain on the hull.
    #[serde(default)]
    pub weather: Option<WeatherSoundTuning>,
}

/// The drawn wind's drift on a world: snow,
/// sand, dust or ash blown low over the ground, in the traced mist. Its
/// colour (lit by the sky and the sun), its density per thousand units at
/// the ground in a full gust, and the height it thins by (e).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DriftTuning {
    pub color: [f32; 3],
    pub density: f32,
    pub height: f32,
}

/// The weather's sound ([`crate::ambience`]) and its drawn wind: each
/// world's wind (its strength and how gusty, 0 to 1, and the bearing it
/// blows toward in degrees, 0 along +Z; a world without air has none), the
/// drift it blows low over the ground where the world names one, and the
/// layers' gains: the wind at full strength, the air rushing past from
/// `rush[0]` to `rush[1]` velocity units, the rain at full fall, and its
/// patter on the hull outside and inside the shell. A layer plays the pak's
/// recording named for it, else a loop synthesized for it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WeatherSoundTuning {
    pub winds: BTreeMap<u32, [f32; 3]>,
    #[serde(default)]
    pub drift: BTreeMap<u32, DriftTuning>,
    pub wind: f32,
    pub rush: [f32; 2],
    pub rush_level: f32,
    pub rain: f32,
    pub hull: f32,
    pub hull_inside: f32,
    #[serde(default)]
    pub wind_recording: Option<String>,
    #[serde(default)]
    pub rain_recording: Option<String>,
    #[serde(default)]
    pub hull_recording: Option<String>,
    /// The thunder after each of the modern sky's flashes: its recordings,
    /// and the gain of a near strike's (a far roll's is half, and duller).
    #[serde(default)]
    pub thunder: Vec<String>,
    #[serde(default)]
    pub thunder_level: f32,
}

/// The shared reverb: each world's preset (`default` for the rest), the
/// share of each placed sound sent to it (`send`; `retail` for retail's own
/// sounds), and how the walls around the camera scale its tail (`enclosure`:
/// the wet's factor in the open and walled in).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReverbTuning {
    pub presets: BTreeMap<String, ReverbPreset>,
    pub worlds: BTreeMap<u32, String>,
    pub default: String,
    pub send: f32,
    pub retail: f32,
    pub enclosure: [f32; 2],
}

/// A place's reverb: its decay to -60 dB in seconds, how dark its tail is
/// (0 to 0.95), and how loud.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReverbPreset {
    pub seconds: f32,
    pub damping: f32,
    pub wet: f32,
}

impl ReverbTuning {
    /// World `world`'s preset.
    pub fn preset(&self, world: u32) -> Option<&ReverbPreset> {
        let name = self.worlds.get(&world).unwrap_or(&self.default);
        self.presets.get(name)
    }
}

/// A placed sound's things: the actors and furniture of `models` (by name),
/// or every actor of `kind` (`force_field`, `barrier`), each playing
/// `place`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SourceTuning {
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub place: PlacedTuning,
}

/// A placed sound: its loops (one picked per place), heard to `range`
/// world units at `gain` of the ambience's level, and `shots`, one-shots
/// from the same place every `every` seconds (a span; a transformer's
/// crackle, lava's pops). For the crystals' rings, `attack`: each ring
/// rises over that many seconds and then follows its crystal's flare down,
/// lasting as long as the flash (none: a one-shot at its full loudness),
/// and `most`, the loudest a ring gets: its `gain` carries it far, and near
/// it stops there.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PlacedTuning {
    pub sounds: Vec<String>,
    pub range: f32,
    pub gain: f32,
    #[serde(default)]
    pub shots: Vec<String>,
    #[serde(default)]
    pub every: Option<[f32; 2]>,
    #[serde(default)]
    pub attack: Option<f32>,
    #[serde(default)]
    pub most: Option<f32>,
}

/// A bed: its recordings, its gain, and its sweeteners: one-shot details
/// placed around the listener now and then (a far rumble, a creak), every
/// `every` seconds when it gives its own span. With `light`, the
/// recordings' level and pitch at the world's darkest light on retail's
/// tint schedule, following it (Artoria's drone powering down); `under`, a
/// bed playing beneath it all the while; `pulse`, a beat ahead.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BedTuning {
    pub loops: Vec<String>,
    pub gain: f32,
    #[serde(default)]
    pub sweeteners: Vec<String>,
    #[serde(default)]
    pub every: Option<[f32; 2]>,
    #[serde(default)]
    pub light: Option<[f32; 2]>,
    #[serde(default)]
    pub under: Option<String>,
    #[serde(default)]
    pub pulse: Option<PulseTuning>,
}

/// A bed's pulse: `beats`, taken in turn, `per_minute`, at `gain` of the
/// bed's level.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PulseTuning {
    pub beats: Vec<String>,
    pub per_minute: f32,
    pub gain: f32,
}

impl AtmosphereTuning {
    /// World `world`'s bed, if it has one.
    pub fn bed(&self, world: u32) -> Option<(&str, &BedTuning)> {
        let name = self.worlds.get(&world)?;
        self.beds.get(name).map(|bed| (name.as_str(), bed))
    }
}

/// The thrusters' trails ([`crate::scene_trails`]): the vehicles' exhausts, and what their puffs look like.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TrailTuning {
    pub exhausts: Vec<TrailExhaust>,
    pub looks: Vec<TrailLook>,
}

/// A model's exhausts: each of `points` a group of its vertex slots, whose
/// middle as drawn the puffs leave from (an empty group: the model's
/// origin), pushed `push` world units a second away from the same group of
/// `from`, if any; in look `look`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TrailExhaust {
    pub name: String,
    pub points: Vec<Vec<i16>>,
    #[serde(default)]
    pub from: Vec<Vec<i16>>,
    #[serde(default)]
    pub push: f32,
    pub look: u8,
}

/// What a trail's puffs look like: their colour (0 to 1, lit as the mist
/// is), their density at birth (per thousand units of view, thinning to
/// none), how many seconds they last, their radius at birth and at the end
/// (world units), how fast they rise (world units a second), and how far
/// apart they leave a moving exhaust.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TrailLook {
    pub color: [f32; 3],
    pub density: f32,
    pub seconds: f32,
    pub radius: [f32; 2],
    pub rise: f32,
    pub spacing: f32,
}

impl TrailTuning {
    /// The exhausts of the model named `name`.
    pub fn exhausts_of<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a TrailExhaust> {
        self.exhausts
            .iter()
            .filter(move |exhaust| glob(exhaust.name.as_bytes(), name.as_bytes()))
    }
}

impl ModernTuning {
    /// The built-in values.
    pub fn defaults() -> Self {
        Self::read(None).expect("the built-in tuning reads")
    }

    /// The built-in values with `overrides` (a tuning file's text) over them.
    pub fn read(overrides: Option<&str>) -> Result<Self, String> {
        let mut value: Value = serde_json::from_str(DEFAULTS).map_err(|error| error.to_string())?;
        if let Some(text) = overrides {
            let over: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
            merge(&mut value, over);
        }
        let fog_value = value.get("fog").cloned().unwrap_or(Value::Null);
        let fog: FogTuning =
            serde_json::from_value(fog_value.clone()).map_err(|error| format!("fog: {error}"))?;
        let sky_value = value.get("sky").cloned().unwrap_or(Value::Null);
        let sky: SkyTuning =
            serde_json::from_value(sky_value.clone()).map_err(|error| format!("sky: {error}"))?;
        let water_value = value.get("water").cloned().unwrap_or(Value::Null);
        let water: WaterTuning = serde_json::from_value(water_value.clone())
            .map_err(|error| format!("water: {error}"))?;
        let mut world_fog = BTreeMap::new();
        let mut world_fog_dark = BTreeMap::new();
        let mut world_sky = BTreeMap::new();
        let mut world_water = BTreeMap::new();
        if let Some(worlds) = value.get("worlds").and_then(Value::as_object) {
            for (key, world) in worlds {
                let number: u32 = key
                    .parse()
                    .map_err(|_| format!("worlds: \"{key}\" is not a world number"))?;
                if let Some(unknown) = world.as_object().and_then(|fields| {
                    fields.keys().find(|field| {
                        !["fog", "fog_dark", "sky", "water"].contains(&field.as_str())
                    })
                }) {
                    return Err(format!("worlds.{key}: unknown field \"{unknown}\""));
                }
                let mut merged = fog_value.clone();
                if let Some(over) = world.get("fog") {
                    merge(&mut merged, over.clone());
                }
                if let Some(over) = world.get("fog_dark") {
                    let mut dark = merged.clone();
                    merge(&mut dark, over.clone());
                    let tuned = serde_json::from_value(dark)
                        .map_err(|error| format!("worlds.{key}.fog_dark: {error}"))?;
                    world_fog_dark.insert(number, tuned);
                }
                let tuned = serde_json::from_value(merged)
                    .map_err(|error| format!("worlds.{key}.fog: {error}"))?;
                world_fog.insert(number, tuned);
                let mut merged = sky_value.clone();
                if let Some(over) = world.get("sky") {
                    merge(&mut merged, over.clone());
                }
                let tuned = serde_json::from_value(merged)
                    .map_err(|error| format!("worlds.{key}.sky: {error}"))?;
                world_sky.insert(number, tuned);
                let mut merged = water_value.clone();
                if let Some(over) = world.get("water") {
                    merge(&mut merged, over.clone());
                }
                let tuned = serde_json::from_value(merged)
                    .map_err(|error| format!("worlds.{key}.water: {error}"))?;
                world_water.insert(number, tuned);
            }
        }
        if let Some(unknown) = value.as_object().and_then(|fields| {
            fields.keys().find(|field| {
                ![
                    "fog",
                    "sky",
                    "water",
                    "materials",
                    "thermal",
                    "headlight",
                    "crystals",
                    "round",
                    "ultra",
                    "trails",
                    "atmosphere",
                    "lighting",
                    "bounce",
                    "worlds",
                ]
                .contains(&field.as_str())
            })
        }) {
            return Err(format!("unknown field \"{unknown}\""));
        }
        let materials =
            serde_json::from_value(value.get("materials").cloned().unwrap_or(Value::Null))
                .map_err(|error| format!("materials: {error}"))?;
        let thermal = serde_json::from_value(value.get("thermal").cloned().unwrap_or(Value::Null))
            .map_err(|error| format!("thermal: {error}"))?;
        let headlight =
            serde_json::from_value(value.get("headlight").cloned().unwrap_or(Value::Null))
                .map_err(|error| format!("headlight: {error}"))?;
        let crystals =
            serde_json::from_value(value.get("crystals").cloned().unwrap_or(Value::Null))
                .map_err(|error| format!("crystals: {error}"))?;
        let round = serde_json::from_value(value.get("round").cloned().unwrap_or(Value::Null))
            .map_err(|error| format!("round: {error}"))?;
        let ultra = serde_json::from_value(value.get("ultra").cloned().unwrap_or(Value::Null))
            .map_err(|error| format!("ultra: {error}"))?;
        let trails = serde_json::from_value(value.get("trails").cloned().unwrap_or(Value::Null))
            .map_err(|error| format!("trails: {error}"))?;
        let atmosphere =
            serde_json::from_value(value.get("atmosphere").cloned().unwrap_or(Value::Null))
                .map_err(|error| format!("atmosphere: {error}"))?;
        let lighting =
            serde_json::from_value(value.get("lighting").cloned().unwrap_or(Value::Null))
                .map_err(|error| format!("lighting: {error}"))?;
        let bounce = serde_json::from_value(value.get("bounce").cloned().unwrap_or(Value::Null))
            .map_err(|error| format!("bounce: {error}"))?;
        Ok(Self {
            bounce,
            fog,
            world_fog,
            world_fog_dark,
            sky,
            world_sky,
            water,
            world_water,
            materials,
            thermal,
            headlight,
            crystals,
            round,
            ultra,
            trails,
            atmosphere,
            lighting,
        })
    }

    /// The fog for world `world`.
    pub fn fog(&self, world: u32) -> FogTuning {
        self.world_fog.get(&world).copied().unwrap_or(self.fog)
    }

    /// The fog's `far` and `past` of every world, which the traced
    /// remaster's game sees by and a replay keeps.
    pub fn traced_far(&self) -> TracedFar {
        let pair = |fog: &FogTuning| [fog.far, fog.past];
        let all = pair(&self.fog);
        TracedFar {
            all,
            worlds: self
                .world_fog
                .iter()
                .map(|(&world, fog)| (world, pair(fog)))
                .filter(|(_, own)| *own != all)
                .collect(),
        }
    }

    /// The fog for world `world` once it is `dark` of the way into its dark
    /// profile (0 to 1, Artoria's once its generators are down): its own,
    /// blended toward that where it has one.
    pub fn fog_at(&self, world: u32, dark: f32) -> FogTuning {
        let fog = self.fog(world);
        match self.world_fog_dark.get(&world) {
            Some(darker) if dark > 0.0 => fog.mix(darker, dark.min(1.0)),
            _ => fog,
        }
    }

    /// The sky for world `world`.
    pub fn sky(&self, world: u32) -> SkyTuning {
        self.world_sky.get(&world).copied().unwrap_or(self.sky)
    }

    /// The water for world `world`.
    pub fn water(&self, world: u32) -> WaterTuning {
        self.world_water.get(&world).copied().unwrap_or(self.water)
    }

    /// The bounce light.
    pub fn bounce(&self) -> BounceTuning {
        self.bounce
    }

    pub fn materials(&self) -> &MaterialTuning {
        &self.materials
    }

    pub fn thermal(&self) -> &ThermalTuning {
        &self.thermal
    }

    pub fn headlight(&self) -> &HeadlightTuning {
        &self.headlight
    }

    pub fn crystals(&self) -> &CrystalTuning {
        &self.crystals
    }

    pub fn round(&self) -> &RoundTuning {
        &self.round
    }

    pub fn trails(&self) -> &TrailTuning {
        &self.trails
    }

    pub fn atmosphere(&self) -> &AtmosphereTuning {
        &self.atmosphere
    }

    pub fn ultra(&self) -> &UltraTuning {
        &self.ultra
    }

    /// The numbers of lighting preset `preset`.
    pub fn lighting(&self, preset: crate::modern_settings::Lighting) -> LightingTuning {
        match preset {
            crate::modern_settings::Lighting::Remaster => self.lighting.remaster,
            crate::modern_settings::Lighting::EyeCandy => self.lighting.eye_candy,
        }
    }
}

/// `over` merged into `value`: objects key by key, anything else replaced.
fn merge(value: &mut Value, over: Value) {
    match (value, over) {
        (Value::Object(fields), Value::Object(over)) => {
            for (key, item) in over {
                merge(fields.entry(key).or_insert(Value::Null), item);
            }
        }
        (value, over) => *value = over,
    }
}

thread_local! {
    static TUNING: Rc<ModernTuning> = Rc::new(ModernTuning::defaults());
}

/// The tuning in force: the built-in values.
pub fn current() -> Rc<ModernTuning> {
    TUNING.with(Rc::clone)
}
#[cfg(test)]
mod tests {
    use super::*;

    /// A file changes only what it names: the defaults fill the rest, and a
    /// world's own values sit over the file's.
    #[test]
    fn overrides_merge_over_the_defaults_and_worlds_over_both() {
        let defaults = ModernTuning::defaults();
        let tuned = ModernTuning::read(Some(
            r#"{"fog": {"density": 2.0}, "worlds": {"9": {"fog": {"height": 100.0}}}}"#,
        ))
        .unwrap();
        let everywhere = tuned.fog(2);
        assert_eq!(everywhere.density, 2.0);
        assert_eq!(everywhere.height, defaults.fog(2).height);
        let tabuchi = tuned.fog(9);
        assert_eq!((tabuchi.density, tabuchi.height), (2.0, 100.0));
        assert_eq!(tabuchi.reach, defaults.fog(9).reach);
    }

    /// The traced fog's reach a replay keeps: the default's, and the worlds
    /// whose own differ, each as its fog has it.
    #[test]
    fn the_traced_far_lists_the_worlds_that_differ() {
        let defaults = ModernTuning::defaults();
        let far = defaults.traced_far();
        assert_eq!(far.all, [defaults.fog.far, defaults.fog.past]);
        for world in 0..32 {
            let fog = defaults.fog(world);
            assert_eq!(far.of(world), [fog.far, fog.past], "world {world}");
        }
        assert!(far.worlds.values().all(|own| *own != far.all));
    }

    /// Each world's water is as clear and as reflective as its own liquid:
    /// the snow world's clearest, the toxic green murky and shining, lava
    /// opaque and glowing; a world the defaults leave out (one without
    /// water) takes the global values, and a file's world entry merges over
    /// the defaults' one.
    #[test]
    fn worlds_take_their_own_water() {
        let defaults = ModernTuning::defaults();
        let caspadril = defaults.water(23);
        let tabuchi = defaults.water(9);
        let altair = defaults.water(5);
        let shaktahh = defaults.water(17);
        assert!(caspadril.clarity > tabuchi.clarity && tabuchi.clarity > altair.clarity);
        assert!(altair.clarity > shaktahh.clarity);
        assert!(altair.reflect > tabuchi.reflect);
        assert_eq!(defaults.water(16), defaults.water(99));
        // Shak'tahh's water tile is lava: it glows.
        assert!(defaults.water(17).glow > 0.0 && altair.glow == 0.0);
        let tuned = ModernTuning::read(Some(r#"{"worlds": {"5": {"water": {"clarity": 500.0}}}}"#))
            .unwrap();
        assert_eq!(tuned.water(5).clarity, 500.0);
        assert_eq!(tuned.water(5).reflect, altair.reflect);
        assert_eq!(tuned.water(5).rings, altair.rings);
    }

    /// Model names take the first pattern they match, stars standing for
    /// any run: the turbo's flame is hot, a mutant's arm warm, a gun arm a
    /// machine's, a rock cold, and a name nothing matches a model's. The
    /// ACAM's gun pods and the hovercraft's hull are the hull's heat, its
    /// fans an engine's (the gun plan).
    #[test]
    fn model_names_take_their_first_matching_heat() {
        let defaults = ModernTuning::defaults();
        let thermal = defaults.thermal();
        let heat = |name: &str| thermal.heat_of(name);
        assert_eq!(heat("turbospin"), 0.95);
        assert_eq!(heat("mutleftarm"), 0.7);
        assert_eq!(heat("gunarm1"), 0.45);
        assert_eq!(heat("powerup12"), 0.6);
        assert_eq!(heat("offlight"), 0.3);
        assert_eq!(heat("rhinoeyelight"), 0.85);
        assert_eq!(heat("boulder02"), 0.22);
        assert_eq!(heat("silo2a"), heat("acam"));
        assert_eq!(heat("hoverjet1"), heat("acam"));
        assert_eq!(heat("scatfan"), 0.95);
        assert_eq!(heat("unlisted"), thermal.models);
        assert_eq!(thermal.listed("unlisted"), None);
        assert!(thermal.is_gun("silo3a") && thermal.is_gun("truckturret"));
        assert!(!thermal.is_gun("f03silo1") && !thermal.is_gun("acam"));
        assert!(thermal.is_gun("towergun2") && !thermal.is_gun("gunstrut01"));
        assert!(thermal.is_wreck("rhinodead") && thermal.is_wreck("damagemedium"));
        assert_eq!(heat("damagemedium"), heat("deadtorso"));
        assert_eq!(
            defaults.materials().of("silo1"),
            defaults.materials().of("acam")
        );
        assert!(glob(b"*a*b", b"xaxxb") && !glob(b"*a*b", b"xaxxbx"));
        assert!(glob(b"a*", b"a") && glob(b"*", b"") && !glob(b"a", b""));
    }

    /// The ACAM's tyres are rubber over their wheels' metal, the truck's
    /// hub its tyre's side wall in its top half; other sprites have none.
    #[test]
    fn the_wheels_sprites_take_their_own_material() {
        let defaults = ModernTuning::defaults();
        let materials = defaults.materials();
        let wheel = materials.of("huskywheelsimple");
        let (tread, none) = materials.of_sprite(195).unwrap();
        assert!(tread.metal == 0.0 && tread.rough > 0.5 && none.is_none());
        assert_eq!(materials.of_sprite(197), Some((wheel, None)));
        let (hub, wall) = materials.of_sprite(198).unwrap();
        assert_eq!((hub, wall), (wheel, Some((0.5, tread))));
        assert_eq!(materials.of_sprite(221), None);
        let tuned = ModernTuning::read(Some(
            r#"{"materials": {"sprites": [[7, 2.0, 0.0, [1.5, -1.0, 0.5]]]}}"#,
        ))
        .unwrap();
        let clamped = |metal, rough| Material { metal, rough };
        assert_eq!(
            tuned.materials().of_sprite(7),
            Some((clamped(1.0, 0.02), Some((1.0, clamped(0.0, 0.5)))))
        );
        assert_eq!(tuned.materials().of_sprite(195), None);
    }

    /// Vehicles and machines take a metal, creatures and rock none, and a
    /// file's own list replaces the built-in one whole.
    #[test]
    fn models_take_their_material_by_name() {
        let defaults = ModernTuning::defaults();
        let acam = defaults.materials().of("acam");
        assert!(acam.metal > 0.0 && acam.rough < 1.0);
        assert_eq!(defaults.materials().of("mutant").metal, 0.0);
        assert_eq!(defaults.materials().of("boulder01").metal, 0.0);
        let tuned = ModernTuning::read(Some(
            r#"{"materials": {"names": [["boulder*", 1.0, 0.0]]}}"#,
        ))
        .unwrap();
        assert_eq!(
            tuned.materials().of("boulder01"),
            Material {
                metal: 1.0,
                rough: 0.02
            }
        );
        assert_eq!(tuned.materials().of("acam").metal, 0.0);
        // The crystals glow in their sprites' colours; a tuning file can
        // quench them. The warpgate keeps retail's look: its arcs light
        // nothing and span no field (2026-10-23, the warpgate plan's first
        // step), though a tuning file can give it both.
        assert_eq!(
            defaults.materials().glow_of("token02", 0).unwrap().color,
            None
        );
        assert_eq!(defaults.materials().glow_of("gatebod", 595), None);
        assert_eq!(defaults.materials().glow_of("acam", 0), None);
        // The buggy glows from its turbo's blue flame (321) and streaks
        // (602) only, not from its rear lamps (585 red, 586 white).
        assert!(defaults.materials().glow_of("wheelmode1a", 321).is_some());
        assert!(defaults.materials().glow_of("wheelmode2a", 602).is_some());
        assert_eq!(defaults.materials().glow_of("wheelmode1a", 585), None);
        assert_eq!(defaults.materials().glow_of("wheelmode1a", 586), None);
        // The VTOL glows from its jets' flames (602, the turbo's streaks),
        // each jet a light of its own.
        let jets = defaults.materials().glow_of("vtol2", 602).unwrap();
        assert!(jets.gather.is_some_and(|apart| apart < 92.0));
        assert_eq!(defaults.materials().glow_of("vtol2", 327), None);
        // The exhausts leave trails: smoke behind
        // the turbo, vapour behind the jets, in both presets. The
        // hovercraft's fans leave none: it has no turbo
        // (2026-10-27).
        let trails = defaults.trails();
        assert_eq!(trails.exhausts_of("scatfan").count(), 0);
        for (name, look) in [
            ("wheelmode1a", 0),
            ("wheelmode2a", 0),
            ("vtol2", 1),
            ("jetmode2", 1),
        ] {
            let exhausts: Vec<_> = trails.exhausts_of(name).collect();
            assert_eq!(exhausts.len(), 1, "{name}");
            assert_eq!(exhausts[0].look, look, "{name}");
            assert!(trails.looks.get(usize::from(look)).is_some());
        }
        for preset in [
            crate::modern_settings::Lighting::Remaster,
            crate::modern_settings::Lighting::EyeCandy,
        ] {
            assert!(defaults.lighting(preset).trails > 0.0);
        }
        let quenched = ModernTuning::read(Some(r#"{"materials": {"glows": []}}"#)).unwrap();
        assert_eq!(quenched.materials().glow_of("token02", 0), None);
        assert_eq!(defaults.materials().field_of("gatebod"), None);
        // Every emitter as retail drew it, unless the tuning names it.
        assert!(defaults.materials().emitters.is_empty());
        assert_eq!(defaults.materials().emitter_of("titleinfestation"), 1.0);
        let signs = ModernTuning::read(Some(
            r#"{"materials": {"names": [], "emitters": [["sign*", 3.0], ["dark", -1.0]]}}"#,
        ))
        .unwrap();
        assert_eq!(signs.materials().emitter_of("sign02"), 3.0);
        assert_eq!(signs.materials().emitter_of("dark"), 0.0);
        assert_eq!(signs.materials().emitter_of("acam"), 1.0);
        let fielded = ModernTuning::read(Some(
            r#"{"materials": {"fields": [["gatebod", 0.6, [0.45, 0.7, 1.0]]]}}"#,
        ))
        .unwrap();
        assert!(fielded.materials().field_of("gatebod").is_some());
        assert_eq!(fielded.materials().field_of("gate"), None);
        // Its lightning is redrawn (the plan's second step), not the outer
        // model's (`gate` only draws `gatebod`); an empty list keeps
        // retail's arcs.
        assert_eq!(
            defaults
                .materials()
                .gate_of("gatebod")
                .map(|(look, _)| look),
            Some(0)
        );
        assert_eq!(defaults.materials().gate_of("gate"), None);
        let plain = ModernTuning::read(Some(r#"{"materials": {"gates": []}}"#)).unwrap();
        assert_eq!(plain.materials().gate_of("gatebod"), None);
        let tinted = ModernTuning::read(Some(
            r#"{"materials": {"glows": [["lamp*", 0.5, 300.0, [1.0, 0.5, 2.0]]]}}"#,
        ))
        .unwrap();
        assert_eq!(
            tinted.materials().glow_of("lamppost", 0),
            Some(crate::modern_tuning::Glow {
                strength: 0.5,
                reach: 300.0,
                color: Some([1.0, 0.5, 1.0]),
                gather: None,
            })
        );
    }

    /// The sky stays retail's painted one unless switched, everywhere or
    /// for one world, which keeps the rest of the defaults.
    /// Pelagia's rain waits for its night (its modern sky since Stage 2);
    /// every other world's weather keeps to the modern sky.
    #[test]
    fn pelagias_rain_waits_for_the_night() {
        let tuning = ModernTuning::defaults();
        let rain = tuning.sky(1).precip;
        assert_eq!(rain.kind, PrecipKind::Rain);
        assert!(rain.with_night && rain.density > 0.0);
        assert_eq!(tuning.sky(1).style, SkyStyle::Modern);
        for world in (0..=33).filter(|world| *world != 1) {
            assert!(!tuning.sky(world).precip.with_night, "world {world}");
        }
    }

    #[test]
    fn the_modern_sky_waits_until_switched_per_world() {
        let defaults = ModernTuning::defaults();
        // Stage 2: the 22 approved recipes are on; the Brain (0, 19) keeps
        // retail's turning clouds.
        for world in (2..=24).filter(|world| *world != 19) {
            assert_eq!(defaults.sky(world).style, SkyStyle::Modern, "world {world}");
        }
        assert_eq!(defaults.sky(19).style, SkyStyle::Painted);
        let tuned = ModernTuning::read(Some(
            r#"{"worlds": {"19": {"sky": {"style": "modern", "zenith": [0.2, 0.4, 0.8],
                "clouds": {"shape": "storm", "cover": 0.8}}}}}"#,
        ))
        .unwrap();
        assert_eq!(tuned.sky(19).style, SkyStyle::Modern);
        assert_eq!(tuned.sky(0).style, SkyStyle::Painted);
        assert_eq!(tuned.sky(19).zenith, Some([0.2, 0.4, 0.8]));
        // A world's block merges key by key into the shared one.
        assert_eq!(tuned.sky(19).clouds.shape, CloudShape::Storm);
        assert_eq!(tuned.sky(19).clouds.cover, 0.8);
        assert_eq!(tuned.sky(19).clouds.base, defaults.sky(19).clouds.base);
        assert_eq!(tuned.sky(19).sun, defaults.sky(19).sun);
        // The sun's shadows soften with any sky, cast from 2.25 degrees
        // each way unless tuned, wider than its drawn disc; airless worlds
        // have no clouds.
        assert_eq!(defaults.sky(2).sun.radius, 1.0);
        assert_eq!(defaults.sky(2).sun.shadow, 2.25);
        assert_eq!(defaults.sky(14).clouds.cover, 0.0);
        let hard = ModernTuning::read(Some(r#"{"sky": {"sun": {"shadow": 0.0}}}"#)).unwrap();
        assert_eq!(hard.sky(9).sun.shadow, 0.0);
        // The flat keys of before are refused, not ignored.
        assert!(ModernTuning::read(Some(r#"{"sky": {"modern": true}}"#)).is_err());
        assert!(ModernTuning::read(Some(r#"{"sky": {"clouds": 0.5}}"#)).is_err());
    }

    /// Remaster keeps the look as tuned, so its frame takes no grade; Eye
    /// candy grades it and strengthens reflections, shafts and glows. A
    /// file changes either preset.
    #[test]
    fn the_lighting_presets_read_their_numbers() {
        use crate::modern_settings::Lighting;
        let defaults = ModernTuning::defaults();
        let remaster = defaults.lighting(Lighting::Remaster);
        assert!(!remaster.grades());
        assert_eq!(
            [remaster.reflections, remaster.shafts, remaster.glows],
            [1.0; 3]
        );
        let candy = defaults.lighting(Lighting::EyeCandy);
        assert!(candy.grades());
        assert!(candy.exposure < 1.0 && candy.saturation > 1.0 && candy.curve > 0.0);
        assert!(candy.reflections > 1.0 && candy.shafts > 1.0 && candy.glows > 1.0);
        let tuned = ModernTuning::read(Some(r#"{"lighting": {"eye_candy": {"saturation": 1.4}}}"#))
            .unwrap();
        assert_eq!(tuned.lighting(Lighting::EyeCandy).saturation, 1.4);
        assert_eq!(tuned.lighting(Lighting::EyeCandy).exposure, candy.exposure);
        assert!(ModernTuning::read(Some(r#"{"lighting": {"neon": {}}}"#)).is_err());
    }

    /// Misspelt names and values of the wrong kind are refused, not
    /// ignored.
    #[test]
    fn a_file_that_does_not_fit_is_refused() {
        assert!(ModernTuning::read(Some(r#"{"fog": {"densty": 2.0}}"#)).is_err());
        assert!(ModernTuning::read(Some(r#"{"fgo": {}}"#)).is_err());
        assert!(ModernTuning::read(Some(r#"{"worlds": {"nine": {}}}"#)).is_err());
        assert!(ModernTuning::read(Some(r#"{"worlds": {"9": {"lava": {}}}}"#)).is_err());
        assert!(
            ModernTuning::read(Some(r#"{"worlds": {"9": {"water": {"clearness": 1.0}}}}"#))
                .is_err()
        );
        assert!(ModernTuning::read(Some(r#"{"fog": {"density": "thick"}}"#)).is_err());
        assert!(ModernTuning::read(Some(r#"{"thermal": {"names": [["a", "hot"]]}}"#)).is_err());
        assert!(ModernTuning::read(Some("{")).is_err());
    }
}
