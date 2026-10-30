#version 450
// The traced world into the frame's target, with its depth, so the raster's
// later draws (blended effects, the HUD) land over and behind it as they
// would over retail's world. The colour is the resolve's, at the render
// size, or an upscaler's at the display's; the depth is the display's. The
// mist lies over both, read from its grid at each pixel's depth; where the
// tracer met nothing it lies over the raster's sky (retail's backdrop and
// clear colour, drawn before the world), which the blend keeps behind it.
// The painted sky takes no clouds of ours: the hybrid of 2026-10-22, clouds
// over the painting's open top, seamed against it (the sky plan). The
// modern sky's clouds also lie over the world wherever it lies beyond them:
// a peak in their layer, or the view from a flight into one.
// Over both falls the modern sky's snow or rain (pt_weather.glsl), and over
// that lie the fields of energy across the active warpgates, under the
// raster's arcs, and the struck crystals' little lightning arcs.
// With the eye under the water (the remastered camera) the view is the
// water's instead (under_water_view).
#extension GL_EXT_scalar_block_layout : require
#extension GL_GOOGLE_include_directive : require

#define FRAME_BINDING 3
#include "pt_frame.glsl"
#include "pt_sky.glsl"
#include "pt_weather.glsl"

layout(set = 0, binding = 0) uniform sampler2D color_image;
layout(set = 0, binding = 1) uniform sampler2D depth_image;
// The mist along the view: its light so far, and how much still shows, at
// the far end of each slice.
layout(set = 0, binding = 2) uniform sampler2D mist;
// The modern sky's clouds along the view, at a quarter of its size: their
// light, and how much of the sky behind shows (pt_sky_clouds.comp pass 0).
layout(set = 0, binding = 4) uniform sampler2D sky_clouds;
// Where along the view they lie (pt_sky.glsl's clouds_before).
layout(set = 0, binding = 5) uniform sampler2D sky_clouds_away;
layout(location = 0) in vec2 v_texture;
layout(location = 0) out vec4 o_color;

#include "pt_arcs.glsl"

// Value noise in two dimensions, 0 to 1, wrapping every `period` cells
// across its first coordinate (round the field).
float field_noise(vec2 p, float period) {
    vec2 cell = floor(p);
    vec2 f = p - cell;
    f = f * f * (3.0 - 2.0 * f);
    float n[4];
    for (int k = 0; k < 4; k++) {
        vec2 corner = cell + vec2(k & 1, k >> 1);
        corner.x = mod(corner.x, period);
        n[k] = float(field_hash(uint(int(corner.x) + 4096)
                                + field_hash(uint(int(corner.y) + 4096))) >> 8) / 16777216.0;
    }
    return mix(mix(n[0], n[1], f.x), mix(n[2], n[3], f.x), f.y);
}

// The fields of energy across the active warpgates along the view through
// this pixel, in front of the world's `reach` (view depth), premultiplied:
// a sheet that swirls round a bright middle and brightens at its rim.
vec4 fields_along(float reach) {
    vec4 light = vec4(0.0);
    if (frame.thermal > 0.5) {
        return light;
    }
    vec3 direction = native_direction(v_texture * frame.native - 0.5);
    float time = frame.water_clock / 50.0;
    for (int i = 0; i < int(frame.field_count); i++) {
        Field field = frame.fields[i];
        vec3 normal = cross(field.across, field.up);
        float facing = dot(direction, normal);
        if (abs(facing) < 1.0e-6) {
            continue;
        }
        float t = dot(field.centre, normal) / facing;
        vec3 point = direction * t;
        if (t <= frame.near || dot(point, frame.forward) >= reach) {
            continue;
        }
        vec3 offset = point - field.centre;
        vec2 at = vec2(dot(offset, field.across) / dot(field.across, field.across),
                       dot(offset, field.up) / dot(field.up, field.up));
        float s = length(at);
        if (s >= 1.0) {
            continue;
        }
        // Bands swirling round the middle, turning faster near it.
        float turn = atan(at.y, at.x) / 6.28318530718 + 0.5;
        float swirl = field_noise(vec2(turn * 12.0 + time * (0.6 + 0.8 * (1.0 - s)) + s * 3.0,
                                       s * 6.0 - time * 1.3), 12.0);
        float fine = field_noise(vec2(turn * 32.0 - time * 1.7, s * 16.0 + time * 2.1), 32.0);
        // Thin bright filaments where the noise crosses its middle.
        float filament = pow(1.0 - abs(2.0 * swirl - 1.0), 4.0);
        float flicker = 0.85 + 0.15 * sin(time * 31.0 + float(i) * 2.0);
        float core = exp(-s * s * 7.0);
        float rim = smoothstep(0.6, 0.85, s) * (1.0 - smoothstep(0.85, 1.0, s));
        float energy = (0.12 + 0.9 * filament + 0.25 * fine * fine) * (1.0 - 0.5 * s)
            + 1.0 * core + 0.25 * rim;
        float alpha = clamp(energy * field.strength * flicker, 0.0, 0.9)
            * (1.0 - smoothstep(0.7, 1.0, s));
        vec3 color = mix(field.color, vec3(1.0), 0.6 * core);
        light = vec4(light.rgb + (1.0 - light.a) * color * alpha,
                     light.a + (1.0 - light.a) * alpha);
    }
    return light;
}

// Port option, the remastered camera under the water: V2000's wobble
// (software_frame.rs's WetFrame, from its retail underwater projectors),
// each point moved round a small circle two pixels of a 480-line view
// across, its phase turning once every 128 clock ticks and once every 64
// such pixels along the diagonal: the texel of `image` shown at `uv`, as
// retail's, a whole pixel's (of the image's), never a blend of several, so
// the view stays sharp. Here, after the temporal passes and the upscalers,
// neither smears it; the HUD comes after.
ivec2 wobbled(vec2 uv, ivec2 size) {
    vec2 lines = vec2(480.0 * frame.display.x / max(frame.display.y, 1.0), 480.0);
    vec2 at = uv * lines;
    float phase = 6.28318530718 * (frame.water_clock + 2.0 * (at.x + at.y)) / 128.0;
    vec2 moved = round(2.0 * vec2(cos(phase), sin(phase)) / lines * vec2(size));
    return clamp(ivec2(uv * vec2(size)) - ivec2(moved), ivec2(0), size - 1);
}

// The view with the eye under the water: no sky, cloud, weather or air. A
// view that met nothing is the water's (the resolve laid it); the grid
// holds the sun's light the water scatters along the view
// (pt_fog_inject.comp), added in linear light; the whole view wobbles. The
// fields and arcs lie over it unwobbled, and so does the depth the
// raster's later draws, unwobbled too, are tested against.
void under_water_view() {
    ivec2 shown_at = wobbled(v_texture, textureSize(color_image, 0));
    vec2 uv = (vec2(shown_at) + 0.5) / vec2(textureSize(color_image, 0));
    ivec2 depth_size = textureSize(depth_image, 0);
    float depth = texelFetch(depth_image, clamp(ivec2(uv * vec2(depth_size)), ivec2(0),
                                                depth_size - 1), 0).r;
    float z = depth >= 1.0 ? frame.volume_far : depth_distance(depth);
    float slice = depth_slice(z, frame.volume_far);
    vec3 sun = mix(vec3(0.0), sample_volume(mist, uv, slice - 0.5).rgb, clamp(slice, 0.0, 1.0));
    vec3 traced = texelFetch(color_image, shown_at, 0).rgb;
    vec3 color = frame.guides > 0.5 ? traced : from_transport(traced);
    vec3 seen = to_srgb(max(color, vec3(0.0)) + sun);
    float here = texture(depth_image, v_texture).r;
    vec4 field = fields_along(depth_distance(here));
    vec3 view = native_direction(v_texture * frame.native - 0.5);
    vec3 arcs = frame.spark_count > 0.5
        ? crystal_arcs(view, depth_distance(here) / max(dot(view, frame.forward), 1.0e-3))
        : vec3(0.0);
    vec3 shown = field.rgb + (1.0 - field.a) * (seen + arcs);
    o_color = vec4(to_srgb(tonemap(to_linear(shown))), 1.0);
    gl_FragDepth = here;
}

void main() {
    if (frame.eye_under > 0.5) {
        under_water_view();
        return;
    }
    float depth = texture(depth_image, v_texture).r;
    bool sky = depth >= 1.0;
    vec4 field = fields_along(sky ? 1.0e9 : depth_distance(depth));
    vec3 view = native_direction(v_texture * frame.native - 0.5);
    vec3 arcs = frame.spark_count > 0.5
        ? crystal_arcs(view, sky ? 1.0e9 : depth_distance(depth) / max(dot(view, frame.forward), 1.0e-3))
        : vec3(0.0);
    // Below the horizon the sky shows only where the rays' reach ends short
    // of the ground (an eye high up, above the fog's top), and the ground
    // there would lie past 1.6 F, gone into the fog's colour: so the sky
    // behind the clouds there takes that colour, as the ground beside it
    // does, its edge one pixel soft.
    float pixel_rise = fwidth(view.y);
    float below = sky && frame.distance_fog > 0.5
        ? clamp(0.5 - view.y / max(pixel_rise, 1.0e-6), 0.0, 1.0)
        : 0.0;
    vec4 fog = vec4(0.0, 0.0, 0.0, 1.0);
    bool misted = frame.mist_density > 0.0 || frame.distance_fog > 0.5 || frame.drift_density > 0.0;
    if (misted) {
        float z = sky ? frame.volume_far : depth_distance(depth);
        // Slice k's value lies at its far end, at slice coordinate k + 1.
        float slice = depth_slice(z, frame.volume_far);
        fog = sample_volume(mist, v_texture, slice - 0.5);
        fog = mix(vec4(0.0, 0.0, 0.0, 1.0), fog, clamp(slice, 0.0, 1.0));
    } else if (sky && frame.sky_modern < 0.5 && field.a <= 0.0 && arcs == vec3(0.0)) {
        discard;
    }
    if (sky && frame.sky_modern < 0.5) {
        // Blended: the mist's light, and the sky through what still shows;
        // the painted sky takes only its tuned share of the mist.
        fog = mix(vec4(0.0, 0.0, 0.0, 1.0), fog, frame.mist_sky);
        // The field over the mist over the sky behind, or below the
        // horizon the fog's colour.
        vec3 behind = fog_colour(fog) * (fog.a * below);
        o_color = vec4(field.rgb + (1.0 - field.a) * (fog.rgb + behind) + arcs,
                       1.0 - (1.0 - field.a) * fog.a * (1.0 - below));
        gl_FragDepth = 1.0;
        return;
    }
    if (sky) {
        // The modern sky takes the same share of the mist as the painted:
        // the whole of it veiled dark clouds in the world's fog colour.
        fog = mix(vec4(0.0, 0.0, 0.0, 1.0), fog, frame.mist_sky);
    }
    // The resolve's light (the upscaler's, the anti-aliasing's), back to
    // retail's tones, uncapped, for the work below, which is tuned in them.
    vec3 traced = texture(color_image, v_texture).rgb;
    vec3 color = to_srgb(frame.guides > 0.5 ? traced : from_transport(traced));
    // A flash of lightning briefly lights everything a little, in its
    // colour (the trace lights what faces it).
    vec4 flash = lightning_flash();
    vec3 flashed = frame.lightning_color * (flash.w * frame.lightning_light);
    color += (color * 0.4 + 0.03) * flashed;
    color = max(color, vec3(0.0));
    vec3 seen = color * fog.a + fog.rgb;
    // The world's distance from the eye along the view, as the fog counts.
    float reach = sky ? 1.0e9
                      : depth_distance(depth) / max(dot(view, frame.forward), 1.0e-3);
    float beyond = 0.0;
    if (!sky && frame.distance_fog > 0.5) {
        // Past the fog's end what still shows (ground and hills above the
        // fog's top) fades out by 1.6 times it, into the fog's own colour
        // along the view, as retail's far terrain fades into its far
        // colour: the world's 65,536-unit wrap never shows its repeats.
        beyond = smoothstep(frame.fog_end, 1.6 * frame.fog_end, reach);
        seen = mix(seen, fog_colour(fog), beyond);
    }
    // The sky below the horizon takes the fog's colour, as the ground
    // beside it does (`below`).
    seen = mix(seen, fog_colour(fog), below);
    if (!sky && frame.precip_kind > 1.5 && frame.precip_kind < 2.5) {
        // The drops striking the ground near the eye, lit as the rain is.
        vec3 toward = native_direction(v_texture * frame.native - 0.5);
        float along = depth_distance(depth) / max(dot(toward, frame.forward), 1.0e-3);
        vec3 point = toward * along;
        vec3 facing = cross(dFdx(point), dFdy(point));
        float up = abs(facing.y) / max(length(facing), 1.0e-6);
        float splash = drop_splashes(point, up, along);
        if (splash > 0.0) {
            vec3 light = min((frame.ambient * 2.0 + frame.sun_color * 0.5) * frame.precip_color,
                             vec3(1.0));
            seen += light * (0.55 * splash * fog.a);
        }
    }
    // The whole modern sky's clouds, at the display's size, the upscaler
    // never having seen them drift: over the traced sky (the gradient with
    // the art the world keeps) all of them, over the world the share that
    // lies nearer, the world faded into the fog taking them as the sky
    // does, so they meet the far skyline without a seam (laid before the
    // mist and the fade, both wiped them there). They are lit by a flash,
    // and veiled by the sky's share of the mist as far as they lie, so a
    // flight through one sees it, not the haze beyond.
    vec4 clouds = frame.sky_modern > 0.5 ? texture(sky_clouds, v_texture)
                                         : vec4(0.0, 0.0, 0.0, 1.0);
    if (clouds.a < 1.0) {
        vec4 placed = texture(sky_clouds_away, v_texture);
        vec4 before = sky ? clouds : clouds_before(clouds, placed, reach, beyond);
        vec4 veil = vec4(0.0, 0.0, 0.0, 1.0);
        if (misted) {
            float z = min(min(placed.a * CLOUD_DISTANCE_UNIT, reach) * dot(view, frame.forward),
                          frame.volume_far);
            float slice = depth_slice(z, frame.volume_far);
            veil = mix(veil, sample_volume(mist, v_texture, slice - 0.5),
                       clamp(slice, 0.0, 1.0) * frame.mist_sky);
        }
        vec3 light = before.rgb + (before.rgb * 0.4 + 0.03 * (1.0 - before.a)) * flashed;
        seen = seen * before.a + light * veil.a + veil.rgb * (1.0 - before.a);
    }
    // A near strike's bolt shows under the clouds.
    if (sky && flash.w > 0.0) {
        seen += lightning_bolt(view) * fog.a;
    }
    if (frame.precip_kind > 0.5) {
        vec3 direction = native_direction(v_texture * frame.native - 0.5);
        float along = sky ? 1.0e9
                          : depth_distance(depth) / max(dot(direction, frame.forward), 1.0e-3);
        vec4 fall = precipitation(direction, along);
        seen = seen * (1.0 - fall.a) + fall.rgb;
    }
    vec3 shown = field.rgb + (1.0 - field.a) * (seen + arcs * fog.a);
    // Into the frame: light past retail's full brightness rolls off to white
    // (the tonemap); infrared vision's grey stays as the camera made it, with a sensor's
    // grain: each output pixel's own, new each frame, a few levels of 255.
    if (frame.thermal > 0.5) {
        uint seed = field_hash(uint(gl_FragCoord.x) + field_hash(uint(gl_FragCoord.y)
                               + field_hash(uint(frame.frame_index))));
        float grain = (float(seed >> 8) / 16777216.0 - 0.5) * (6.0 / 255.0);
        shown = clamp(shown + grain, 0.0, 1.0);
    }
    o_color = vec4(frame.thermal > 0.5 ? shown : to_srgb(tonemap(to_linear(shown))), 1.0);
    gl_FragDepth = depth;
}
