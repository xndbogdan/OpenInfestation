// A metal face's fans in Vulkan's raster (vk_metal.vert): retail's colour,
// as vk_textured.frag (TEXTURED) or vk_flat.frag lays it, with the modern
// sky mirrored in it at the face's roughness, as the tracer's metal mirrors
// the sky (pt_trace.comp's path pass): as metallic as it is, the pixel takes
// its colour times the sky's light along GGX's lobe in place of the light
// retail gave it. The raster has the sky alone: the share of the lobe that
// goes below the horizon (to the ground and what stands on it, which the
// tracer's rays meet) keeps retail's light, as does a pixel under the
// water's plane.
#extension GL_EXT_scalar_block_layout : require
#extension GL_GOOGLE_include_directive : require

#define FRAME_SET 1
#define FRAME_BINDING 0
#include "pt_frame.glsl"
#include "pt_glossy.glsl"
#include "pt_wet.glsl"

layout(push_constant) uniform Push {
    vec2 scale;
    float depth_test;
    float sky_cube;
} push;
#ifdef TEXTURED
layout(set = 0, binding = 0) uniform sampler2D u_texture;
#endif
// The modern sky in every direction, its clouds over it (pt_sky_clouds.comp's
// cube), where push.sky_cube is 1.
layout(set = 1, binding = 1) uniform samplerCube sky_cube;
layout(location = 0) in vec3 v_texture;
layout(location = 1) in vec4 v_diffuse;
layout(location = 2) in vec4 v_fog;
layout(location = 3) in float v_depth;
layout(location = 4) in vec2 v_native;
layout(location = 5) in vec4 v_normal;
layout(location = 6) in vec4 v_albedo;
layout(location = 7) flat in uvec2 v_material;
layout(location = 0) out vec4 o_color;

// How many directions of the lobe are read: the same set at every pixel and
// in every frame, so nothing shimmers, each read from the cube's level whose
// texels are as wide as its share of the lobe.
const uint LOBE_SAMPLES = 16u;

// The sky's light along `direction` as the raster shows the sky: the modern
// sky's cube at `level`, through the tonemap as vk_sky.comp lays it (and the
// puddles take it); else the painted sky's colour.
vec3 sky_seen(vec3 direction, float level) {
    if (push.sky_cube > 0.5) {
        return tonemap(to_linear(textureLod(sky_cube, direction, level).rgb));
    }
    return to_linear(frame.sky);
}

// The cube's level for the direction `toward` of the lobe of `rough` that
// reflects `view` off `normal`: its share of LOBE_SAMPLES over the lobe's
// density there (GGX's visible normals', through the reflection) against a
// texel's solid angle at the cube's first level (Colbert and Krivanek's
// filtered importance sampling).
float lobe_level(vec3 view, vec3 normal, vec3 toward, float rough) {
    float alpha = max(rough * rough, 1.0e-3);
    float a2 = alpha * alpha;
    float n_v = max(dot(normal, -view), 1.0e-4);
    float n_h = max(dot(normal, normalize(toward - view)), 0.0);
    float d = a2 / (3.14159265 * pow(n_h * n_h * (a2 - 1.0) + 1.0, 2.0));
    float masking = 2.0 * n_v / (n_v + sqrt(a2 + (1.0 - a2) * n_v * n_v));
    float density = masking * d / (4.0 * n_v);
    float side = float(textureSize(sky_cube, 0).x);
    float texel = 4.0 * 3.14159265 / (6.0 * side * side);
    return max(0.5 * log2(1.0 / (float(LOBE_SAMPLES) * density * texel)) + 1.0, 0.0);
}

void main() {
    if (push.depth_test > 0.5 && v_depth > 1.0) discard;
    gl_FragDepth = clamp(v_depth, 0.0, 1.0);
#ifdef TEXTURED
    vec4 texel = texture(u_texture, v_texture.xy / v_texture.z);
    vec4 color = texel * v_diffuse;
#else
    vec4 texel = vec4(1.0);
    vec4 color = v_diffuse;
#endif
    // The material where the pixel lies in its sprite (above the split, the
    // second: materials.sprites), by the preset's reflections, as the
    // tracer reads it.
    float rhw = v_normal.w;
    uint split = v_material.y & 0xffffu;
    uint word = split != 0u && v_albedo.a / rhw < float(split) / 65536.0 ? v_material.x >> 16
                                                                          : v_material.x;
    float metal = min(float(word & 0xffu) / 255.0 * frame.reflect_gain, 1.0);
    float rough = max(float((word >> 8) & 0xffu) / 255.0 / max(frame.reflect_gain, 1.0e-3),
                      0.02);
    vec3 normal = normalize(v_normal.xyz / rhw);
    vec3 view = native_direction(v_native - 0.5);
    float soaked = 0.0;
    if (frame.wet_models > 0.0) {
        bool coat;
        wet_model(normal.y, (v_material.y >> 16) != 0u, metal, rough, soaked, coat);
    }
    // Where the pixel lies, from its depth (1 - near / z): under the water's
    // plane the sky does not reach it.
    float z = frame.near / max(1.0 - v_depth, 1.0e-7);
    float height = frame.eye.y + view.y * z / max(dot(view, frame.forward), 1.0e-4);
    if (metal > 0.0 && height >= frame.water_level) {
        // The rain darkens bare rubber as it soaks it, in retail's tones,
        // as the ground.
        float darker = 1.0 - RAIN_DARKEN * soaked;
        vec3 own = to_linear(color.rgb * darker);
        vec3 albedo = to_linear(texel.rgb * v_albedo.rgb / rhw * darker);
        vec3 seen = vec3(0.0);
        float known = 0.0;
        for (uint i = 0u; i < LOBE_SAMPLES; i++) {
            vec2 xi = vec2((float(i) + 0.5) / float(LOBE_SAMPLES),
                           float(bitfieldReverse(i)) * 2.3283064365386963e-10);
            float weight;
            vec3 toward = glossy_direction(view, normal, rough, xi, weight);
            // Into the ground, retail's light stays; into the surface, no
            // light, as the tracer's.
            if (weight > 0.0 && toward.y <= 0.0) {
                continue;
            }
            known += 1.0;
            if (weight > 0.0) {
                seen += sky_seen(toward, lobe_level(view, normal, toward, rough)) * weight;
            }
        }
        known /= float(LOBE_SAMPLES);
        seen /= float(LOBE_SAMPLES);
        color.rgb = to_srgb(own * (1.0 - metal * known) + albedo * metal * seen);
    }
#ifdef TEXTURED
    color.rgb = mix(v_fog.rgb, color.rgb, v_fog.a);
#endif
    o_color = color;
}
