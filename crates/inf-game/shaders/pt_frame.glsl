// The modern mode's frame block, which every tracer pass shares, and the
// view direction through a pixel. Define FRAME_BINDING before including it
// (and FRAME_SET, where its set is not the first).
// The layout is trace.rs's frame block, float for float.

// A field of energy across an active warpgate (scene_field.rs): its centre
// (eye-relative), its half-width and half-height as vectors in its plane,
// how strongly it shows and its colour.
struct Field {
    vec3 centre;
    vec3 across;
    vec3 up;
    float strength;
    vec3 color;
};

// A lamp's beam (modern_tuning.rs's LampLook): the light 1,000 units out on
// a surface square to it, the half-angle across and the share of it at full
// strength, the top edge above the axis and how soft it is, over how much
// below the axis it thins (angles in radians), where its falloff eases and
// where it ends, and how far across it casts its shadows from (0: a point).
struct LampLook {
    float intensity;
    float spread;
    float inner;
    float cutoff;
    float soft;
    float hot;
    float near;
    float reach;
    float size;
};

// A vehicle's lamp (world_scene.rs's place_lamps): where it is
// (eye-relative), the way its beam points, the beam's across and up, its
// colour, and its beam (0 a headlamp's, 1 a rear lamp's).
struct Lamp {
    vec3 position;
    vec3 axis;
    vec3 across;
    vec3 up;
    vec3 color;
    float look;
};

// The most lamps a frame lights: on each of the nearest four vehicles, two
// headlamps and a pair of rear lamps.
#define MAX_LAMPS 16
// A struck crystal's pairs of arc anchors (crystal_flares.rs's ARC_PAIRS).
#define ARC_PAIRS 6
// The points of the craft's wake through the clouds (cloud_wake.rs's
// WAKE_POINTS).
#define WAKE_POINTS 12

#ifndef FRAME_SET
#define FRAME_SET 0
#endif
layout(set = FRAME_SET, binding = FRAME_BINDING, scalar) uniform Frame {
    vec3 right;
    vec3 up;
    vec3 forward;
    vec2 focal;
    vec2 centre;
    vec2 native;
    vec2 target;
    float near;
    float far;
    vec3 sun_direction;
    vec3 sun_color;
    vec3 ambient;
    vec3 fog_color;
    float fog_start;
    float fog_end;
    vec3 sky;
    // The eye's world position, X and Z within the 65,536-unit wrap.
    vec3 eye;
    vec2 grid_anchor;
    float grid_spacing;
    vec3 grid_outside;
    // 0: no grid, 1: models only, 2: models and ground.
    float grid_mode;
    // Counts traced frames, for the bounce rays' random numbers.
    float frame_index;
    // 0 when the last frame's history cannot be followed (a new size, the
    // first frame): the bounce light starts again.
    float history_valid;
    // The last traced frame's view, to find where a point was then.
    vec3 previous_right;
    vec3 previous_up;
    vec3 previous_forward;
    vec2 previous_focal;
    vec2 previous_centre;
    vec3 previous_eye;
    // The wave clock the water's heights were taken at.
    float water_clock;
    // The water's triangles come first: a model's start after them.
    float model_first;
    // This frame's sub-pixel offset of the primary rays, in render pixels,
    // for an upscaler (zero without one).
    vec2 jitter;
    // The display's size: `target` is the size the tracer renders at, which
    // an upscaler brings up to it.
    vec2 display;
    // How far apart neighbouring pixels' rays are for texture filtering, in
    // render pixels: one, or half a display pixel under an upscaler (the
    // sharper mip bias its temporal resolve wants).
    float texel_step;
    // 1 when the resolve also writes Ray Reconstruction's guides.
    float guides;
    // The haze over the ground and the water: retail's far terrain colour,
    // which it turns to past the fog's end, the distance where it is
    // complete (where retail's far terrain ends), and how quickly it
    // thickens towards there.
    vec3 far_color;
    float haze_reach;
    float haze_rate;
    // The mist: its density (per thousand units) at and below its base
    // height (0: none), the height over which it thins by e, the base
    // height, the share of the density it keeps at any height, its forward
    // scattering, the sun's share of its light, and how strongly retail's
    // lights glow in it.
    float mist_density;
    float mist_height;
    float mist_base;
    float mist_floor;
    float mist_anisotropy;
    float mist_sun;
    float mist_glow;
    // The share of the mist laid over the raster's sky.
    float mist_sky;
    // The mist's grid: cells across, down and deep; the depth its slices
    // reach from `near`, now and in the last frame; this frame's point
    // within a cell.
    vec3 volume;
    float volume_far;
    float previous_volume_far;
    vec3 volume_jitter;
    // The ground's grid points a side and their spacing (0: no ground).
    float ground_points;
    float ground_spacing;
    // The water: its plane's height, how far from the eye its mesh reaches
    // (0: no water) and its texture (-1: none).
    float water_level;
    float water_reach;
    float water_texture;
    // Thermal vision (1: on): the heat of the sky, the water and the
    // ground, the lamp at the eye's strength, how much retail's lights warm
    // what they light, the heat of glowing surfaces, and the share of the
    // way the camera's range follows each frame's.
    float thermal;
    float thermal_sky;
    float thermal_water;
    float thermal_ground;
    float thermal_lamp;
    float thermal_lights;
    float thermal_glowing;
    float thermal_rate;
    // The frame's deposited lights in the light source buffer.
    float light_count;
    // The painted sky's triangles start after the water's; the models'
    // (model_first) after the sky's.
    float sky_first;
    // The sky (1: the renderer's own, pt_sky.glsl; else retail's painted
    // backdrop): its colour overhead and at the horizon, how soon the one
    // gives way to the other, the sun's glow and disc, how many stars show,
    // and the warm glow low toward the sun.
    float sky_modern;
    vec3 sky_zenith;
    vec3 sky_horizon;
    float sky_curve;
    float sky_glow;
    float sky_disc;
    float sky_stars;
    float sky_horizon_glow;
    // Retail's splashes in the splash buffer, and their look: how steeply
    // their rings tilt the water, how white their foam is, how far a ring
    // spreads over a splash's life and how wide it is.
    float splash_count;
    float splash_rings;
    float splash_foam;
    float splash_spread;
    float splash_width;
    // The masked sprites' triangles start after the opaque models'.
    float cutout_first;
    // The cosine of the angular radius of the sun's drawn disc (the modern
    // sky), and of the one its shadows are cast from: a primary hit's
    // shadow ray aims at a point of that, so shadows soften with distance.
    float sun_cone;
    float shadow_cone;
    // The spray's sources in the spray buffer (their drops follow), and how
    // brightly lights under the water (a blast's fireballs) glow through it.
    float spray_count;
    float eruption_flash;
    // The water: how far one sees into it before its own colour hides the
    // bottom, the least share of its surroundings it mirrors, and how
    // brightly it glows where it is lava (0: water).
    float water_clarity;
    float water_reflect;
    float water_glow;
    // The fields of energy across the active warpgates, the nearest first.
    float field_count;
    Field fields[2];
    // The clouds (pt_sky.glsl): how much of the sky they cover, their
    // shape (0 cumulus, 1 stratus, 2 storm, 3 ash), the layer's bottom and
    // top as world heights, how wide their largest cells are and how fast
    // they drift (world units a clock tick), their sunlit and shaded
    // colours, and how much of the sun their shadows take from the ground.
    float cloud_cover;
    float cloud_shape;
    float cloud_base;
    float cloud_top;
    float cloud_size;
    float cloud_speed;
    vec3 cloud_lit;
    vec3 cloud_shade;
    float cloud_shadow;
    // How far along a primary hit's normal its shadow rays start (the
    // traced menu's low-poly planets; 0 for the world).
    float shadow_lift;
    // The lighting preset's reflections: how much more a metal mirrors its
    // surroundings, and how much more sharply (1: as tuned).
    float reflect_gain;
    // The weather's fall (pt_weather.glsl): 0 none, 1 snow, 2 rain, 3
    // ash, 4 embers; the
    // share of its cells that hold a flake or a drop; how fast it falls
    // and drifts, in world units a second; a flake's or a drop's size and a
    // streak's length, in world units; and its colour.
    float precip_kind;
    float precip_density;
    float precip_fall;
    vec2 precip_wind;
    float precip_size;
    float precip_length;
    vec3 precip_color;
    // The storm: the colour of veins glowing inside the clouds (black:
    // none), and the lightning's flashes a second, colour, and how much a
    // flash brightens the world (pt_sky.glsl's lightning_flash).
    vec3 cloud_veins;
    // The colour of light from below on the clouds' undersides (black:
    // none), and how many times longer than wide they stretch across X.
    vec3 cloud_under;
    float cloud_streak;
    // How far off the clouds fade into the horizon's haze, by e every this
    // far.
    float cloud_haze;
    // How much of the light inside them scatters on (1: water clouds), and
    // how far they round off toward the top (1: domes, 0: full bodies).
    float cloud_albedo;
    float cloud_round;
    float lightning_rate;
    vec3 lightning_color;
    float lightning_light;
    // How much of its colour the bounce light keeps (its brightness kept).
    float bounce_colour;
    // How much of the blended sprites that add light the water and metal
    // mirror (Eye candy's share; 0: none), and how many are in the emitter
    // buffer.
    float effect_mirrors;
    float emitter_count;
    // The distance fog, which the mist's grid carries in a world's view
    // (1: on) in place of the haze and retail's vertex fog: where it starts
    // (a view depth), its optical depth at the fog's end near the ground and
    // how that grows from its start (an exponent), the height above which
    // it thins, over how much height, to what share, and the view depth
    // past which models dissolve.
    float distance_fog;
    float fog_near;
    float fog_depth;
    float fog_curve;
    float fog_top;
    float fog_band;
    float fog_above;
    float fog_dissolve;
    // The shore (the plan's item 2): over how much depth the water fades in
    // from its edge, how far that edge wanders (units of depth), and how
    // strongly a lapping foam line marks it; how high above the water the
    // ground looks damp, and how dark damp ground is (1: no darker). The
    // spray (item 3): the share of what lies behind a clear drop it shows,
    // how much of its liquid's colour it takes, and how much lighter the
    // water a blast churned stays.
    float shore_fade;
    float shore_wobble;
    float shore_lap;
    float wet_height;
    float damp;
    float see_through;
    float drop_tint;
    float churn;
    // The vehicles' lamps: how many, the
    // headlamps' beam and the rear lamps' (modern_tuning.rs's
    // HeadlightTuning), and what they share: how far the ground's facing
    // wraps, how much of their light entering the water glows back in the
    // water's colour, how rough the ripples they glint in are, how much they
    // glow in the air, through the mist or the least air of their own, the
    // water's own colour, which they glow in under the water, and how
    // strongly.
    float lamp_count;
    LampLook lamp_looks[2];
    float lamp_wrap;
    float lamp_water_scatter;
    float lamp_water_gloss;
    float lamp_air;
    float lamp_air_floor;
    vec3 lamp_water_hue;
    float lamp_water_glow;
    Lamp lamps[MAX_LAMPS];
    // The sunk additive sprites' first triangle (trace.rs's glow range,
    // after the masked sprites').
    float glow_first;
    // How strongly a pick-up sunk under the water shows through it (1: as
    // retail's blend adds it).
    float sunk_pickups;
    // The drawn wind's drift low over the ground (0: none): its density per
    // thousand units at the ground now (its gust in it), the height it thins
    // by, the wind's velocity across X and Z in world units a second, and
    // its colour before the light.
    float drift_density;
    float drift_height;
    vec2 drift_wind;
    vec3 drift_color;
    // A world hanging in the modern sky (the homeworld): the direction to
    // its middle, the cosine of its angular radius (0: none), its colour lit
    // by the sun, and its night side's lights.
    vec3 planet_direction;
    float planet_cos;
    vec3 planet_color;
    float planet_lights;
    // The weather's wet (world_weather.rs): the ground's, which darkens it,
    // the models', a sheen on metal and paint, and how far the puddles have
    // filled (none until the ground has soaked a while); 0 to 1.
    float wet_ground;
    float wet_models;
    float puddles;
    // The sparks running through struck crystals (crystal_flares.rs): how
    // many, each where it struck (eye-relative) and its age in seconds, each
    // crystal's reach, each one's light (rgb: the flare's colour for what
    // struck, plasma or laser) and their life in seconds.
    float spark_count;
    vec4 spark_at[4];
    float spark_radius[4];
    vec4 spark_light[4];
    float spark_seconds;
    // Where the player's craft stirs the puddles it drives through: how
    // many places, and each (eye-relative, at the ground) with how hard in
    // w (0: not moving). A wheeled form's wheels on the ground, else its
    // middle. Negative w: a wheel or the hull stirring the water, at its
    // plane (pt_splash.glsl's splashed).
    float wake_count;
    vec4 wakes[6];
    // Where the craft's spray falls back on the puddles (craft_spray.rs):
    // how many rings, each eye-relative at the water with how far through
    // its life it is in w (0 to 1).
    float spray_ring_count;
    vec4 spray_rings[16];
    // Each spark's crystal's middle (eye-relative), which its arcs crawl
    // round (pt_composite.frag's crystal_arcs).
    vec4 spark_middle[4];
    // The pairs of points on each spark's crystal's faces its arcs jump
    // between (crystal_flares.rs's ARC_PAIRS of them), eye-relative.
    vec4 spark_anchors[4 * ARC_PAIRS * 2];
    // How retail's baked ground shade rose with the sun's facing, in display
    // tones (scene_terrain.rs's ground_shade_slope): the ground's
    // reflectance comes from it (pt_trace.comp's ground_reflectance).
    float ground_slope;
    // How strongly the modern sky lights what it shines on (the tuning's
    // sky.light).
    float sky_light;
    // 1 while an upscaler runs (its own history follows the denoiser's).
    float upscaling;
    // The emissive light list's groups the path pass samples
    // (pt_trace.comp's emissive_sample).
    float emissive_groups;
    // Port option, the remastered camera: 1
    // with the eye under the water's live surface (the world draw's own
    // decision; never over lava or under thermal vision), where the water
    // round it is a medium (water_through); and the distance over which it
    // leaves 1/e of what lies beyond, half hidden where Vulkan's raster's
    // murk is (world_scene::MURK_REACH).
    float eye_under;
    float water_murk;
    // The clouds (pt_sky.glsl): how far each one's base and top stand above
    // or below the layer's at most, in world units, and how far the near
    // band reaches, the clouds between the eye and the player's craft it
    // follows (just past the craft; 0 in first person, on the Cranial
    // Implanter's missile and without a craft), which keep it in sight.
    float cloud_rise;
    float cloud_craft;
    // The craft's wake through them (cloud_wake.rs): how many points, each
    // eye-relative with its age in seconds in w, the craft now first.
    float cloud_wake_count;
    vec4 cloud_wake[WAKE_POINTS];
} frame;

// A lamp's light at eye-relative `position`, before the surface's facing
// and its shadow: its beam's shape round its axis (full across the inner
// share of its spread and up to its cutoff; below the axis it thins over
// `hot`, as a low beam does, so the ground just behind or ahead, near and so
// lit the most by the falloff, does not outshine the ground the beam
// reaches), falling off with the square of the distance (eased near the
// lamp) to nothing at its reach. `toward` and `distance` lead back to the
// lamp. Its strength, which lamp_beam colours.
float lamp_strength(Lamp lamp, vec3 position, out vec3 toward, out float distance) {
    LampLook look = frame.lamp_looks[int(lamp.look)];
    vec3 offset = position - lamp.position;
    distance = length(offset);
    toward = -offset / max(distance, 1.0e-3);
    vec3 local = vec3(dot(offset, lamp.across), dot(offset, lamp.up), dot(offset, lamp.axis));
    if (local.z <= 0.0 || distance >= look.reach) {
        return 0.0;
    }
    float side = abs(atan(local.x, local.z));
    float rise = atan(local.y, local.z);
    float shape = 1.0 - smoothstep(look.spread * look.inner, look.spread, side);
    shape *= 1.0 - smoothstep(look.cutoff, look.cutoff + look.soft, rise);
    float below = max(-rise, 0.0) / look.hot;
    shape *= exp(-below * below);
    float window = 1.0 - pow(distance / look.reach, 4.0);
    float falloff = 1.0e6 / (distance * distance + look.near * look.near);
    return look.intensity * shape * falloff * window * window;
}

vec3 lamp_beam(Lamp lamp, vec3 position, out vec3 toward, out float distance) {
    return lamp.color * lamp_strength(lamp, position, toward, distance);
}

// A lamp's beam as what glows in it takes it (the air, the water): close to
// the lamp the falloff makes it far brighter than anywhere it lights, which
// would glow as a blot there, so it stops at 1.5 times its light 1,000
// units out.
vec3 glowing_beam(Lamp lamp, vec3 position, out vec3 toward, out float distance) {
    return min(lamp_beam(lamp, position, toward, distance),
               vec3(1.5 * frame.lamp_looks[int(lamp.look)].intensity));
}

// How far the tracer's rays look in a world: four times the largest fog's
// end (12,544), fixed so the skyline stays put while the fog's end moves
// (each world's first second, slow frames).
const float TRACE_REACH = 50176.0;

// The rays' reach: a world's, or four times a scene's own far end where
// that lies farther (the traced main menu's planets, past 50,000).
float trace_reach() {
    return max(TRACE_REACH, 4.0 * frame.far);
}

// The view direction through native coordinate `native`.
vec3 native_direction(vec2 native) {
    vec2 view = (native - frame.centre) / frame.focal;
    return normalize(frame.right * view.x - frame.up * view.y + frame.forward);
}

// The view direction through render pixel `pixel`, jittered. The raster
// draws native coordinate x at target (x + 0.5) * k, so the pixel's centre is
// native (pixel + 0.5) / k - 0.5.
vec3 pixel_direction(vec2 pixel) {
    return native_direction((pixel + 0.5 + frame.jitter) * frame.native / frame.target - 0.5);
}

// The raster's depth for a traced surface at view distance z: the TL depth
// the game gives a point there, 1 - near/z with the near plane at 64 and no
// far plane (the D3D consumers' depth from 1/z), taken a thousandth farther.
// Retail drew a glow, decal or particle lying on a surface with the same
// projection as the surface, so its LESSEQUAL test let the later draw win
// the tie; the traced depth only approximates the raster's rounded one, and
// without the slack such overlays flicker as the camera moves.
float raster_depth(float z) {
    return clamp(1.0 - frame.near / max(z * 1.001, frame.near), 0.0, 1.0);
}

// The view distance behind a raster depth (raster_depth's inverse).
float depth_distance(float depth) {
    return frame.near / max((1.0 - depth) * 1.001, 1.0e-7);
}

// The mist's slices run from `near` to `far`, each a fixed ratio deeper than
// the last: the depth where slice coordinate `slice` (0 to volume.z) lies,
// and the slice coordinate of depth `z`.
float slice_depth(float slice, float far) {
    return frame.near * pow(far / frame.near, slice / frame.volume.z);
}

float depth_slice(float z, float far) {
    return frame.volume.z * log(max(z, frame.near) / frame.near) / log(far / frame.near);
}

// A grid of the mist's cells, its slices stacked down one image, read at
// `uv` across the view and slice coordinate `slice` (cell k's centre at
// k + 0.5), filtered within and between slices.
vec4 sample_volume(sampler2D cells, vec2 uv, float slice) {
    float slices = frame.volume.z;
    float s = clamp(slice - 0.5, 0.0, slices - 1.0);
    float first = floor(s);
    float second = min(first + 1.0, slices - 1.0);
    vec2 inside = clamp(uv, 0.5 / frame.volume.xy, 1.0 - 0.5 / frame.volume.xy);
    vec4 near_cell = textureLod(cells, vec2(inside.x, (first + inside.y) / slices), 0.0);
    vec4 far_cell = textureLod(cells, vec2(inside.x, (second + inside.y) / slices), 0.0);
    return mix(near_cell, far_cell, s - first);
}

// The colour of the grid's `fog` along a view where it hides everything:
// the light it scattered for what it hid, or the far colour where it hid
// almost nothing.
vec3 fog_colour(vec4 fog) {
    return 1.0 - fog.a > 0.05 ? fog.rgb / (1.0 - fog.a) : frame.far_color;
}

// The one pixel of each two by two the path pass traces this frame, turning
// each frame so that four frames cover them all (pt_trace.comp's
// PATHS_PASS, pt_temporal.comp's fill).
ivec2 path_offset() {
    uint turn = uint(frame.frame_index) & 3u;
    return ivec2(turn == 1u || turn == 2u ? 1 : 0, turn == 1u || turn == 3u ? 1 : 0);
}

// The frame's light runs uncapped from the trace to the composite (the full
// path tracing plan's stage 1): a value past 1 is light retail clipped.

// sRGB's transfer both ways, its power segment carried on past 1.
vec3 to_linear(vec3 c) {
    vec3 s = max(c, vec3(0.0));
    return mix(s / 12.92, pow((s + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), s));
}

vec3 to_srgb(vec3 c) {
    vec3 s = max(c, vec3(0.0));
    return mix(s * 12.92, 1.055 * pow(s, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), s));
}

// A light in retail's tones (a shade: 128 is full light) to linear light.
// Past full light (a tint over 128: Terra Nova's sun is 1.0, 1.55, 1.39)
// retail scaled the whole colour down by its brightest channel, keeping its
// hue; so the hue is decoded at full light and the excess made brighter by
// retail's gamma. Decoding each channel past 1 would push the hue apart (that
// sun's green to 2.7 times its red).
vec3 light_to_linear(vec3 c) {
    float peak = max(c.r, max(c.g, c.b));
    if (peak <= 1.0) {
        return to_linear(c);
    }
    return to_linear(c / peak) * pow(peak, 2.2);
}

// The world's sun in full path tracing's units, from retail's tint: a tint
// may pass full light (Terra Nova's 1.0, 1.55, 1.39; Foldrak Noir's 1.4,
// 1.9, 1.9), which retail capped at every lit pixel keeping its hue, so
// such a sun lit whatever faced it half way or more fully. Its hue is
// decoded at full light and its excess kept as it is: retail showed
// min(1, tint N.L + ambient), which linear light under the tonemap follows
// best this way (decoding the excess with retail's gamma made it three or
// four times full light and blew the ground out to white).
vec3 sun_light() {
    vec3 tint = frame.sun_color;
    float peak = max(1.0, max(tint.r, max(tint.g, tint.b)));
    return to_linear(tint / peak) * peak;
}

// Henyey-Greenstein's phase over 4 pi: 1 for light scattered alike every way.
float forward_scatter(float cos_angle, float g) {
    float g2 = g * g;
    return (1.0 - g2) / pow(max(1.0 + g2 - 2.0 * g * cos_angle, 1.0e-4), 1.5);
}

// Retail's flat water: the tint times a shade of 6 of 15 (0044C9D0).
const float WATER_SHADE = 6.0 / 15.0;

// Port option, the remastered camera under the water: the water round the eye as a medium, single scattering in
// closed form. It reads as the same water as Vulkan's raster's murk: the
// water's own colour (its texture's mean, the lamps' hue), lit as the
// tracer lights the water's body from above (retail's shade of 6, by the
// sun and the sky), half of what lies beyond hidden where the murk hides
// half (water_murk).
//
// How much a ray loses per unit of its way through the water, each channel:
// the water's strongest keeps water_murk, the others go sooner by
// how much less of them the water holds (red first in green or blue water).
vec3 water_extinction() {
    vec3 hue = max(frame.lamp_water_hue, vec3(0.02));
    return vec3(max(hue.r, max(hue.g, hue.b))) / hue / max(frame.water_murk, 1.0);
}

// How much of the light reaching it the water scatters, each channel.
vec3 water_albedo() {
    return to_linear(frame.lamp_water_hue * WATER_SHADE);
}

// How forward the water scatters the sun (Henyey-Greenstein's g): it glows
// toward the sun's way in.
const float WATER_FORWARD = 0.3;
// How much longer the sky's light travels down through the water than its
// depth: it comes down within Snell's window (48.6 degrees round straight
// down), whose cosine-weighted mean cosine is 0.84.
const float SKY_SLANT = 1.19;

// The way down through the water's plane (Snell, index 1.333) of a light
// from above toward `toward`, and the share of it the plane lets in
// (Schlick's Fresnel term, 0.02 straight down); none from below the
// horizon.
vec3 water_beam(vec3 toward, out float enters) {
    float rise = toward.y;
    enters = rise > 0.0 ? 1.0 - (0.02 + 0.98 * pow(1.0 - rise, 5.0)) : 0.0;
    return rise > 0.0 ? refract(-toward, vec3(0.0, 1.0, 0.0), 1.0 / 1.333) : -toward;
}

// How deep an eye-relative point lies under the water's plane, with the
// eye under the water (0 above it, or with the eye above the water).
float sunk_depth(vec3 position) {
    return frame.eye_under > 0.5 ? max(frame.water_level - frame.eye.y - position.y, 0.0) : 0.0;
}

// The integral of e^(-a s) from 0 to `len`, each channel.
vec3 water_span(vec3 a, float len) {
    vec3 x = a * len;
    return mix((1.0 - exp(-x)) / a, vec3(len) * (1.0 - 0.5 * x), lessThan(abs(x), vec3(1.0e-3)));
}

// The light the water scatters toward eye-relative `origin` along
// `direction` over `len`, and in `through` the share of the light from
// beyond that reaches `origin`: the sky's light (the sky over the water,
// unshadowed) and, where `sunlit`, the sun's, unshadowed, each dimmed by
// the water above the point it scatters at. The eye's own view takes the
// sun's from the mist's grid instead, with its shadows and shafts
// (pt_fog_inject.comp).
vec3 water_through(vec3 origin, vec3 direction, float len, bool sunlit, out vec3 through) {
    vec3 sigma = water_extinction();
    through = exp(-sigma * len);
    float depth = max(frame.water_level - frame.eye.y - origin.y, 0.0);
    vec3 sky_rate = sigma * SKY_SLANT;
    vec3 light = light_to_linear(max(frame.ambient, vec3(0.0))) * frame.sky_light
        * exp(-sky_rate * depth) * water_span(sigma - sky_rate * direction.y, len);
    float enters;
    vec3 beam = water_beam(frame.sun_direction, enters);
    if (sunlit && enters > 0.0) {
        vec3 sun_rate = sigma / -beam.y;
        light += sun_light() * (enters * forward_scatter(dot(direction, -beam), WATER_FORWARD))
            * exp(-sun_rate * depth) * water_span(sigma - sun_rate * direction.y, len);
    }
    return water_albedo() * sigma * light;
}

// The tonemap: the identity up to TONE_KNEE, then Khronos PBR Neutral's
// shoulder to white, its hue kept and paling only late. PBR Neutral's toe
// (taking 0.04 off every colour) is left out: retail's darks stay where
// they were.
const float TONE_KNEE = 0.8;
const float TONE_PALING = 0.15;
vec3 tonemap(vec3 c) {
    float peak = max(c.r, max(c.g, c.b));
    if (peak <= TONE_KNEE) {
        return c;
    }
    float d = 1.0 - TONE_KNEE;
    float shown = 1.0 - d * d / (peak + d - TONE_KNEE);
    c *= shown / peak;
    float pale = 1.0 - 1.0 / (TONE_PALING * (peak - shown) + 1.0);
    return mix(c, vec3(shown), pale);
}

// The traced colour's form between the resolve and the composite, through
// the anti-aliasing and the upscalers, which take colours from 0 to 1: the
// linear light squeezed below 1 by its brightest channel (Karis's
// reversible tonemap), sRGB-encoded so their edges and blends work in the
// tones they were made for. Ray Reconstruction takes the linear light as it
// is (it is told the colour is HDR).
vec3 to_transport(vec3 c) {
    return to_srgb(c / (1.0 + max(c.r, max(c.g, c.b))));
}

vec3 from_transport(vec3 t) {
    vec3 c = to_linear(min(t, vec3(1.0)));
    return c / max(1.0 - max(c.r, max(c.g, c.b)), 1.0e-3);
}
