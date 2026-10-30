// The ground's grid as the remaster reads it, shared by the tracer
// (pt_trace.comp) and the raster's water and wet ground (vk_mirror.comp):
// its heights, the rain's puddles, the craft's wake through them and the
// lake's shore they meet. Include after pt_frame.glsl and pt_water.glsl,
// with `ground_vertices` (GroundVertex: position, normal, albedo) bound, or
// GROUND_Y(at) defined: the height of the grid's point `at` (X-major), as
// the includer keeps the grid.

#ifndef GROUND_Y
#define GROUND_Y(at) ground_vertices[at].position.y
#endif

// The ground's height under world X/Z between its grid points, bilinear
// (as the mist reads it); the water's plane where there is no ground.
float ground_height(vec2 world) {
    int points = int(frame.ground_points);
    if (points < 2) {
        return frame.water_level;
    }
    vec2 grid_at = mod(world, 65536.0) / frame.ground_spacing;
    ivec2 corner = clamp(ivec2(floor(grid_at)), ivec2(0), ivec2(points - 2));
    vec2 t = clamp(grid_at - vec2(corner), 0.0, 1.0);
    int at = corner.x * points + corner.y;
    float near_row = mix(GROUND_Y(at), GROUND_Y(at + points), t.x);
    float far_row = mix(GROUND_Y(at + 1), GROUND_Y(at + points + 1), t.x);
    return mix(near_row, far_row, t.y);
}

// The rain's puddles (world_weather.rs): water stands where the ground lies
// lower than round it, never on a crest, only in lower pits. How far a point lies below the
// ground 320 units round it sets how soon it fills, the deepest hollows
// first as the puddles fill (frame.puddles: none until the ground has
// soaked a while), each growing from its deepest point; a noise breaks
// their shores. Nearly flat ground above the water's plane only; 0 to 1 at
// `position`.
float puddle_at(vec3 position, vec3 normal) {
    float filled = frame.puddles;
    if (filled <= 0.0 || normal.y < 0.94 || frame.ground_points < 1.5) {
        return 0.0;
    }
    vec2 world = frame.eye.xz + position.xz;
    float ground = ground_height(world);
    // Under the water's plane the ground is the lake's bed, where no
    // puddle stands (puddles only form above water
    // level); by the plane puddle_shore has handed it the lake's look.
    if (ground < frame.water_level) {
        return 0.0;
    }
    float around = 0.0;
    for (int k = 0; k < 6; k++) {
        float a = float(k) * 1.0471976;
        around += ground_height(world + 320.0 * vec2(cos(a), sin(a)));
    }
    float hollow = around / 6.0 - ground;
    float n = 0.65 * wrap_noise(world, 160, 0x50u) + 0.35 * wrap_noise(world, 448, 0x51u);
    float fill = hollow / 32.0 + (n - 0.5) * 0.6;
    // Barely filled, only the deepest hollows reach the level, and even
    // they come in from nothing over the first fill.
    float level = 1.1 - 0.9 * filled;
    return smoothstep(level, level + 0.25, fill) * smoothstep(0.94, 0.985, normal.y)
        * smoothstep(0.0, 0.15, filled);
}

// The craft stirring a puddle it drives through (slightly, from any
// of the wheels in contact with the puddle, 1 to 4): rings
// spreading while it moves, faint and close, from each wheel on the ground
// (a wheelless craft's middle) that stands in a puddle itself.
vec3 waked(vec3 normal, vec3 position) {
    float time = frame.water_clock / 50.0;
    for (int i = 0; i < int(frame.wake_count); i++) {
        vec4 wake = frame.wakes[i];
        if (wake.w <= 0.0) {
            continue;
        }
        vec2 offset = position.xz - wake.xz;
        float distance = length(offset);
        if (distance > 200.0 || abs(position.y - wake.y) > 120.0
            || puddle_at(wake.xyz, vec3(0.0, 1.0, 0.0)) < 0.45) {
            continue;
        }
        for (int k = 0; k < 2; k++) {
            float life = fract(time * 1.6 + 0.5 * float(k) + 0.27 * float(i));
            float x = (distance - (25.0 + 140.0 * life)) / 6.0;
            float strength = wake.w * (1.0 - life) * 0.5;
            normal.xz += offset / max(distance, 1.0) * exp(-x * x) * sin(x * 3.14159265) * strength;
        }
    }
    // Where the spray it throws at speed falls back (craft_spray.rs): a
    // ring from each, spreading to 40 units as it fades. The rings stand
    // where the water does (the drops chose them so).
    for (int i = 0; i < int(frame.spray_ring_count); i++) {
        vec4 ring = frame.spray_rings[i];
        vec2 offset = position.xz - ring.xz;
        float distance = length(offset);
        if (distance > 48.0 || abs(position.y - ring.y) > 40.0) {
            continue;
        }
        float life = ring.w;
        float x = (distance - (4.0 + 36.0 * life)) / 3.0;
        float strength = (1.0 - life) * (1.0 - life) * 0.6;
        normal.xz += offset / max(distance, 1.0) * exp(-x * x) * sin(x * 3.14159265) * strength;
    }
    return normal;
}

// How near water lies to world X/Z: all of it within a tile's width of the
// nearest grid point under the water's plane, none from two.
float water_near(vec2 world) {
    float spacing = frame.ground_spacing;
    vec2 base = (floor(world / spacing) - 1.0) * spacing;
    float nearest = 4.0 * spacing;
    for (int k = 0; k < 16; k++) {
        vec2 corner = base + vec2(k & 3, k >> 2) * spacing;
        if (ground_height(corner) < frame.water_level) {
            nearest = min(nearest, length(corner - world));
        }
    }
    return 1.0 - smoothstep(spacing, 2.0 * spacing, nearest);
}

#include "pt_wet.glsl"

// Wet means darker and smoother, not more reflective (2026-10-28;
// Lagarde's rain in Remember Me): a puddle mirrors as water does, Schlick's
// Fresnel from water's 0.02 (index 1.333), so a chase camera 20 degrees
// above the ground sees about 14% mirrored and the dark wet bed through the
// rest, a mirror only toward the horizon.
const float WATER_F0 = 0.02;
// Soil, sand and grass mostly darken as they soak and stay fairly matte:
// their sun's sheen is a water film's (F0 0.02), broad. The ground is all
// soil, sand, grass and rock (retail's terrain names none hard, and its
// one material, matte); the hard surfaces are models (the pads, platforms,
// bridges and buildings), which the wet coat makes glossy.
const float WET_GROUND_ROUGHNESS = 0.5;

// How far above the water's edge a puddle hands its look to the lake's.
const float PUDDLE_SHORE_BAND = 160.0;

// How far a puddle at an eye-relative point has become the lake's shore
// (a puddle meeting the water looked ugly, so it takes the
// water's colour or a slow gradient): none PUDDLE_SHORE_BAND above the water's edge (where the
// lake's own fade puts it, its wander included), all of it at the edge, and
// only where water lies near; `edge`, the lake's fade at that height
// (water_color's). Within the band the puddle takes the lake's look, as
// shipped games' shore masks hand their standing water to the sea's: at the
// waterline both show the wet bed alone, the bright mirror coming on slowly
// up the bank.
float puddle_shore(vec3 position, out float edge) {
    edge = 1.0;
    if (frame.water_reach <= 0.0 || frame.water_glow > 0.0) {
        return 0.0;
    }
    vec2 world = frame.eye.xz + position.xz;
    float depth = frame.water_level - (frame.eye.y + position.y) + shore_wander(world);
    if (-depth >= PUDDLE_SHORE_BAND) {
        return 0.0;
    }
    edge = frame.shore_fade > 0.0 ? smoothstep(0.0, frame.shore_fade, depth) : 1.0;
    return (1.0 - smoothstep(0.0, PUDDLE_SHORE_BAND, -depth)) * water_near(world);
}

