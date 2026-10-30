#version 460
// vk_primitive.vert for the raster's draws over a traced world (the full
// path tracing plan's stage 7): a half-transparent one (smoke, debris, the
// blended sprites: fog_mode 1) takes the light where each corner stands,
// as the traced world does, in `v_light`: the sun through one shadow ray,
// and the sky's share where the sun does not reach, against both. Retail
// drew them in full light; in a shadow they now darken as the ground under
// them does. Additive and darkening draws (fog_mode 2) are light of their
// own and keep their colour.
#extension GL_EXT_ray_query : require
#extension GL_EXT_scalar_block_layout : require
#extension GL_GOOGLE_include_directive : require

#define FRAME_SET 1
#define FRAME_BINDING 3
#include "pt_frame.glsl"

layout(set = 1, binding = 6) uniform accelerationStructureEXT scene;
layout(push_constant) uniform Push {
    vec2 scale;
    float depth_test;
    // 1: into the fog's colour; 2: fades to nothing.
    float fog_mode;
    vec2 jitter;
} push;
layout(location = 0) in vec3 a_position;
layout(location = 1) in vec3 a_texture;
layout(location = 2) in vec4 a_diffuse;
layout(location = 3) in vec4 a_fog;
layout(location = 0) out vec3 v_texture;
layout(location = 1) out vec4 v_diffuse;
layout(location = 2) out vec4 v_fog;
layout(location = 3) out float v_depth;
layout(location = 4) out vec2 v_native;
// The corner's light against full light, in retail's tones.
layout(location = 5) out float v_light;

// The traced scene's solid instances, what lies outside the view among
// them (pt_scene.glsl's MASK_SOLID).
const uint MASK_SOLID = 0x41u;

float luminance(vec3 light) {
    return dot(light, vec3(0.2126, 0.7152, 0.0722));
}

void main() {
    gl_Position = vec4((a_position.xy + 0.5) * push.scale - 1.0 + push.jitter, 0.0, 1.0);
    v_depth = a_position.z;
    v_texture = a_texture;
    v_diffuse = a_diffuse;
    v_fog = a_fog;
    v_native = a_position.xy + 0.5;
    v_light = 1.0;
    if (push.fog_mode > 1.5 || a_position.z >= 1.0 || a_position.z <= 0.0) {
        return;
    }
    // The corner in the world, from its TL depth (1 - near / z).
    float z = frame.near / max(1.0 - a_position.z, 1.0e-7);
    vec3 toward = native_direction(a_position.xy + 0.5);
    vec3 point = toward * (z / max(dot(toward, frame.forward), 1.0e-3));
    float seen = 0.0;
    if (frame.sun_direction.y > 0.0) {
        rayQueryEXT query;
        rayQueryInitializeEXT(query, scene,
                              gl_RayFlagsTerminateOnFirstHitEXT | gl_RayFlagsOpaqueEXT, MASK_SOLID,
                              point, 2.0, frame.sun_direction, 1.0e6);
        rayQueryProceedEXT(query);
        seen = rayQueryGetIntersectionTypeEXT(query, true) == gl_RayQueryCommittedIntersectionNoneEXT
            ? 1.0
            : 0.0;
    }
    // The sun as the trace takes it (its hue at full light times its
    // peak), and the sky's share (retail's ambient at the world's tuned
    // strength).
    vec3 tint = frame.sun_color;
    float peak = max(1.0, max(tint.r, max(tint.g, tint.b)));
    float sun = luminance(to_linear(tint / peak) * peak);
    float sky = luminance(light_to_linear(frame.ambient)) * frame.sky_light;
    float lit = (sky + sun * seen) / max(sky + sun, 1.0e-4);
    v_light = to_srgb(vec3(lit)).x;
}
