// The splashes on the water, shared by the tracer (pt_trace.comp) and
// Vulkan's raster (vk_mirror.comp): the water's memory (vk_waves.comp,
// docs/rt-plans/water-memory.md), whose waves retail's splashes, the blasts'
// eruptions and the player's craft stir, tilting the surface by their
// slopes, with its foam and the water it churned; and the blasts under the
// water glowing through it. Define SPLASH_BINDING and WAVE_BINDING and
// include it after pt_water.glsl.

// The blasts' eruptions (kind 1, scene_spray.rs): where each rose
// (eye-relative), the seconds since its blast and its size (1 at full
// strength), and how deep under the surface it went off.
struct Splash {
    vec3 position;
    float age;
    float scale;
    float kind;
    float depth;
};
layout(set = 0, binding = SPLASH_BINDING, scalar) readonly buffer Splashes { Splash splashes[]; };

// Its height over the plane, its rise, its foam and its churn
// (vk_waves.comp), one world cell a texel, wrapped with the world.
layout(set = 0, binding = WAVE_BINDING) uniform sampler2D wave_field;

// How much of the water's memory shows at an eye-relative point: all of it
// but the window's rim, none without one (frame.wave_field: a cell's side,
// cells a side).
float wave_share(vec3 position) {
    if (frame.wave_field.y < 0.5) {
        return 0.0;
    }
    float half_side = 0.5 * frame.wave_field.x * frame.wave_field.y;
    float far = max(abs(position.x), abs(position.z));
    return 1.0 - smoothstep(0.8 * half_side, 0.95 * half_side, far);
}

vec4 wave_at(vec2 world) {
    return textureLod(wave_field, world / (frame.wave_field.x * frame.wave_field.y), 0.0);
}

// The most the waves tilt the surface (a slope's length).
const float WAVE_TILT_CAP = 0.8;

// The water's normal tilted by its waves (their slopes, times the tuning's
// `rings`), and `foam` the foam on it (0 to 1), from above or below.
vec3 splashed(vec3 normal, vec3 position, out float foam) {
    foam = 0.0;
    float share = wave_share(position);
    if (share > 0.0) {
        vec2 world = frame.eye.xz + position.xz;
        float side = frame.wave_field.x;
        vec2 slope = vec2(wave_at(world + vec2(side, 0.0)).x - wave_at(world - vec2(side, 0.0)).x,
                          wave_at(world + vec2(0.0, side)).x - wave_at(world - vec2(0.0, side)).x)
            / (2.0 * side) * frame.splash_rings * share;
        float tilt = length(slope);
        if (tilt > WAVE_TILT_CAP) {
            slope *= WAVE_TILT_CAP / tilt;
        }
        normal.xz -= (normal.y < 0.0 ? -1.0 : 1.0) * slope;
        // Thin foam shows as lace: foam_cover's froth answers a young
        // splash's whole foam, and the memory's thins as it spreads.
        foam = sqrt(max(wave_at(world).z, 0.0)) * share;
    }
    return normalize(rained(normal, position));
}

// A blast under the water glows through it for its first second, from a
// ball of light about its fireballs' size: how long, its colour, how far
// (in the water's clarity) its light reaches the surface, and how strongly
// the water scatters it (the tuning's `flash` scales it).
const float GLOW_SECONDS = 1.0;
const vec3 GLOW_COLOR = vec3(1.0, 0.72, 0.38);
const float GLOW_BALL = 150.0;
const float GLOW_REACH = 2.0;
const float GLOW_POWER = 1000.0;

// The blasts under the water seen through its surface: what the water
// scatters towards the eye along the refracted ray (from `start`, `reach`
// long) from each, by the closed form of 1 / distance squared along the
// ray, dimmed by the water on the way and taking its colour with depth.
// Retail's own fireball lights cannot serve: a particle's light shrinks
// with its height over the ground under it, the bed of a deep lake, and
// below 256 retail deposits none. Its brightness is soft-clipped below full
// light, keeping its hue.
vec3 under_glow(vec3 start, vec3 bent, float reach, vec3 body) {
    vec3 glow = vec3(0.0);
    vec3 hue = body / max(max(body.r, max(body.g, body.b)), 1.0e-3);
    for (int i = 0; i < int(frame.splash_count); i++) {
        Splash splash = splashes[i];
        if (splash.depth <= 0.0 || splash.age >= GLOW_SECONDS) {
            continue;
        }
        vec3 toward = splash.position - vec3(0.0, splash.depth, 0.0) - start;
        float size = GLOW_BALL * max(splash.scale, 0.25);
        float along = dot(toward, bent);
        float miss = sqrt(max(dot(toward, toward) - along * along, 0.0) + size * size);
        float seen = (atan((reach - along) / miss) + atan(along / miss)) / miss;
        float path = clamp(along, 0.0, reach) + miss;
        float through = exp(-path / (frame.water_clarity * GLOW_REACH));
        float flash = pow(1.0 - splash.age / GLOW_SECONDS, 2.0) * max(splash.scale, 0.25);
        glow += GLOW_COLOR * mix(hue, vec3(1.0), sqrt(through)) * through * flash * seen;
    }
    float peak = max(glow.r, max(glow.g, glow.b));
    if (peak <= 0.0) {
        return vec3(0.0);
    }
    return glow / peak * (1.0 - exp(-peak * frame.eruption_flash * GLOW_POWER));
}

// The water lighter where the waves' memory holds churn (a blast's
// eruption, the craft's hull ploughing through), as water full of air is.
vec3 churned(vec3 body, vec3 position) {
    if (frame.churn <= 0.0) {
        return body;
    }
    float share = wave_share(position);
    float churn = share > 0.0 ? wave_at(frame.eye.xz + position.xz).w * share : 0.0;
    return mix(body, min(body * 1.8 + vec3(0.04), vec3(1.0)), clamp(churn, 0.0, 1.0) * frame.churn);
}
