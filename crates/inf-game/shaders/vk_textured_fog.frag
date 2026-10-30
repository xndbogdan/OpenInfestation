#version 450
// vk_textured.frag over a traced world: in place of retail's vertex fog, the
// tracer's own fog (its distance fog and mist, lit as the world's) read from
// its grid at the draw's depth, as the composite reads it for the world
// behind, so a blended face before the fog (Stendahl Alpha's crystals, the
// user, 2026-10-23) sinks into it as the ground does. An additive or
// darkening draw fades to nothing in it, the others into its colour; past
// the fog's end both fade out by 1.6 times it, as the world does. With the
// eye under the water (the remastered camera) the water between instead,
// as the world behind takes it (pt_resolve.comp, pt_composite.frag): what
// it leaves of the draw's light and, over a draw that is no light of its
// own, the light it scatters on the way, the sky's and the grid's sun, in
// linear light.
#extension GL_EXT_scalar_block_layout : require
#extension GL_GOOGLE_include_directive : require

#define FRAME_SET 1
#define FRAME_BINDING 3
#include "pt_frame.glsl"

layout(push_constant) uniform Push {
    vec2 scale;
    float depth_test;
    // 1: into the fog's colour; 2: fades to nothing.
    float fog_mode;
} push;
layout(set = 0, binding = 0) uniform sampler2D u_texture;
layout(set = 1, binding = 2) uniform sampler2D mist;
layout(location = 0) in vec3 v_texture;
layout(location = 1) in vec4 v_diffuse;
layout(location = 2) in vec4 v_fog;
layout(location = 3) in float v_depth;
layout(location = 4) in vec2 v_native;
// The light where the draw stands, against full light (vk_primitive_lit.vert).
layout(location = 5) in float v_light;
layout(location = 0) out vec4 o_color;
void main() {
    if (push.depth_test > 0.5 && v_depth > 1.0) discard;
    vec4 color = texture(u_texture, v_texture.xy / v_texture.z) * v_diffuse;
    color.rgb *= v_light;
    // The TL depth is 1 - near / z.
    float z = frame.near / max(1.0 - v_depth, 1.0e-7);
    float slice = depth_slice(z, frame.volume_far);
    vec4 fog = sample_volume(mist, v_native * push.scale * 0.5, slice - 0.5);
    fog = mix(vec4(0.0, 0.0, 0.0, 1.0), fog, clamp(slice, 0.0, 1.0));
    // Past the fog's end by distance from the eye, as the fog counts.
    vec2 uv = v_native * push.scale * 0.5;
    vec3 toward = native_direction(uv * frame.native - 0.5);
    float reach = z / max(dot(toward, frame.forward), 1.0e-3);
    float beyond = smoothstep(frame.fog_end, 1.6 * frame.fog_end, reach);
    if (frame.eye_under > 0.5) {
        vec3 through;
        vec3 scattered = water_through(vec3(0.0), toward, reach, false, through) + fog.rgb;
        vec3 lit = to_linear(clamp(color.rgb, 0.0, 1.0)) * through;
        color.rgb = to_srgb(push.fog_mode > 1.5 ? lit : lit + scattered);
    } else if (push.fog_mode > 1.5) {
        color.rgb *= fog.a * (1.0 - beyond);
    } else {
        color.rgb = mix(clamp(color.rgb, 0.0, 1.0) * fog.a + fog.rgb, fog_colour(fog), beyond);
    }
    gl_FragDepth = clamp(v_depth, 0.0, 1.0);
    o_color = color;
}
