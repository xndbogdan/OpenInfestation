// The splashes on the water, shared by the tracer (pt_trace.comp) and
// Vulkan's raster (vk_mirror.comp): retail's own splashes, each a ring
// spreading from where it fell with foam where it was hit, and the blasts'
// eruptions (their dome, rings and foam, the water they churned and their
// glow under the surface): functions of the world's position, the splash
// buffer and the frame block alone. Define SPLASH_BINDING and include it
// after pt_water.glsl.

// Retail's splashes: where each fell (eye-relative), its age (0 at birth,
// 1 at its end) and its scale, the nearest first; before them the blasts'
// eruptions (kind 1, scene_spray.rs), whose age is the seconds since the
// blast and whose scale is their size (1 at full strength).
struct Splash {
    vec3 position;
    float age;
    float scale;
    float kind;
    // An eruption's blast: how deep under the surface it went off.
    float depth;
};
layout(set = 0, binding = SPLASH_BINDING, scalar) readonly buffer Splashes { Splash splashes[]; };

// The splashes a point may meet, each `i` in the buffer's order: all of
// them, unless the includer walks fewer (vk_mirror.comp's, those that can
// reach its workgroup's pixels).
#ifndef FOR_EACH_SPLASH
#define FOR_EACH_SPLASH(i) for (int i = 0; i < int(frame.splash_count); i++)
#endif

// A blast's eruption on the water (scene_spray.rs's span, then its dome, in
// seconds).
const float ERUPTION_SECONDS = 4.5;
const float DOME_SECONDS = 0.5;

// The surface where a blast in the water erupts, `time` seconds after it at
// `size`: a dome as the blast breaks the surface, rings sent out one after
// another, and a patch of foam that spreads and breaks up as it fades. Its
// light under the surface is its fireballs' own (under_glow).
vec3 erupted(vec3 normal, vec3 position, Splash splash, inout float foam) {
    float time = splash.age;
    float size = max(splash.scale, 0.05);
    vec2 offset = position.xz - splash.position.xz;
    float distance = length(offset);
    float reach = (400.0 + 900.0 * time) * size;
    if (distance > reach) {
        return normal;
    }
    // The tilts fade out before the reach, so neither ends in a rim.
    float edge = 1.0 - smoothstep(0.6 * reach, reach, distance);
    vec2 outward = offset / max(distance, 1.0);
    float dome_radius = 260.0 * size;
    float dome = sin(3.14159265 * clamp(time / DOME_SECONDS, 0.0, 1.0));
    float x = distance / dome_radius;
    normal.xz += outward * x * exp(-x * x) * dome * 1.5 * edge;
    // The dome is white water while it swells.
    foam = max(foam, dome * (1.0 - smoothstep(0.3 * dome_radius, dome_radius, distance)));
    for (int ring = 0; ring < 3; ring++) {
        float since = time - 0.35 * float(ring);
        if (since <= 0.0) {
            break;
        }
        float radius = (80.0 + 700.0 * since) * size;
        float width = (40.0 + 30.0 * since) * size;
        float across = (distance - radius) / width;
        float strength = exp(-1.2 * since) / (1.0 + 0.5 * float(ring));
        normal.xz += outward * exp(-across * across) * sin(across * 3.14159265) * strength
            * frame.splash_rings * 2.0 * edge;
    }
    float patch_radius = (150.0 + 260.0 * sqrt(time)) * size;
    float fade = smoothstep(0.1, 0.35, time) * pow(max(1.0 - time / ERUPTION_SECONDS, 0.0), 1.5);
    vec2 world = (frame.eye.xz + position.xz) / (60.0 * size);
    float broken = 0.5 * value_noise(vec3(world, 0.0)) + 0.3 * value_noise(vec3(world * 2.3, 1.0))
        + 0.2 * value_noise(vec3(world * 5.1, 2.0));
    float threshold = mix(0.25, 0.7, time / ERUPTION_SECONDS);
    float ragged = distance + (broken - 0.5) * 0.5 * patch_radius;
    foam = max(foam, fade * (1.0 - smoothstep(0.5 * patch_radius, patch_radius, ragged))
                         * smoothstep(threshold, threshold + 0.25, broken));
    return normal;
}

// Retail's splashes on the water: each a ring spreading from where it fell
// as it ages, a crest and a trough tilting the surface, fading as it goes,
// with foam where it was hit, out to a share of its ring, while it is
// young; and the blasts' eruptions. `foam` takes the strongest.
const float SPLASH_START = 24.0;
const float FOAM_SHARE = 0.2;
// The most the splashes' rings together tilt the water at a point: several
// overlapping stacked into a swirl of warped mirror round the craft.
const float RING_TILT_CAP = 0.2;

// A retail splash's ring at its age: where it fell, its radius and width,
// its strength and its foam's radius.
struct Ring {
    vec3 position;
    float radius;
    float width;
    float strength;
    float patch_radius;
};

Ring ring_of(Splash splash) {
    Ring ring;
    ring.position = splash.position;
    ring.radius = mix(SPLASH_START, frame.splash_spread, sqrt(splash.age)) * splash.scale;
    ring.width = frame.splash_width * splash.scale;
    ring.strength = (1.0 - splash.age) * (1.0 - splash.age);
    ring.patch_radius = ring.radius * FOAM_SHARE;
    return ring;
}

// Retail's rings a point may meet, in the buffer's order: each splash the
// includer walks, unless it walks its own (vk_mirror.comp's, its
// workgroup's rings made ahead in shared memory).
#ifndef FOR_EACH_RING
#define FOR_EACH_RING(i) FOR_EACH_SPLASH(i) if (splashes[i].kind < 0.5)
#define RING_AT(i) ring_of(splashes[i])
#endif

vec3 splashed(vec3 normal, vec3 position, out float foam) {
    foam = 0.0;
    vec2 rings = vec2(0.0);
    FOR_EACH_SPLASH(i) {
        Splash splash = splashes[i];
        if (splash.kind > 0.5 && abs(position.y - splash.position.y) <= 256.0) {
            normal = erupted(normal, position, splash, foam);
        }
    }
    FOR_EACH_RING(i) {
        Ring ring = RING_AT(i);
        if (abs(position.y - ring.position.y) > 256.0) {
            continue;
        }
        vec2 offset = position.xz - ring.position.xz;
        float distance = length(offset);
        if (distance > ring.radius + 2.0 * ring.width) {
            continue;
        }
        float x = (distance - ring.radius) / ring.width;
        float crest = exp(-x * x) * sin(x * 3.14159265);
        rings += offset / max(distance, 1.0) * crest * ring.strength * frame.splash_rings;
        float patch_radius = ring.patch_radius;
        foam = max(foam, ring.strength
                             * (1.0 - smoothstep(0.4 * patch_radius, patch_radius, distance)));
    }
    // The player's craft through the water, its wheels in it and its hull
    // in or over it (world_scene.rs's craft_wakes, negative strength):
    // rings spreading from each while it moves, wider than a puddle's.
    float time = frame.water_clock / 50.0;
    for (int i = 0; i < int(frame.wake_count); i++) {
        vec4 wake = frame.wakes[i];
        vec2 offset = position.xz - wake.xz;
        float distance = length(offset);
        if (wake.w >= 0.0 || distance > 320.0 || abs(position.y - wake.y) > 64.0) {
            continue;
        }
        for (int k = 0; k < 2; k++) {
            float life = fract(time * 1.2 + 0.5 * float(k) + 0.27 * float(i));
            float x = (distance - (30.0 + 260.0 * life)) / 10.0;
            rings += offset / max(distance, 1.0) * exp(-x * x) * sin(x * 3.14159265)
                * -wake.w * (1.0 - life) * 0.4;
        }
    }
    float tilt = length(rings);
    normal.xz += tilt > RING_TILT_CAP ? rings * (RING_TILT_CAP / tilt) : rings;
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
    FOR_EACH_SPLASH(i) {
        Splash splash = splashes[i];
        if (splash.kind < 0.5 || splash.depth <= 0.0 || splash.age >= GLOW_SECONDS) {
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

// The water a blast churned, lighter where its eruption spread, fading over
// the eruption's span.
vec3 churned(vec3 body, vec3 position) {
    if (frame.churn <= 0.0) {
        return body;
    }
    float churn = 0.0;
    FOR_EACH_SPLASH(i) {
        Splash splash = splashes[i];
        if (splash.kind < 0.5) {
            continue;
        }
        float reach = (400.0 + 900.0 * min(splash.age, ERUPTION_SECONDS)) * max(splash.scale, 0.05);
        float distance = length(position.xz - splash.position.xz);
        float fade = 1.0 - smoothstep(0.0, ERUPTION_SECONDS, splash.age);
        churn = max(churn, fade * (1.0 - smoothstep(0.6 * reach, reach, distance)));
    }
    return mix(body, min(body * 1.8 + vec3(0.04), vec3(1.0)), churn * frame.churn);
}
