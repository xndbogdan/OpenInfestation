#version 450
// Retail's ground again, after the raster's world, where it is what the
// world shows (its depth passes against the world's, placed as
// vk_primitive.vert places it): the colour its fog took, and how much of the
// ground shows through that fog, for the wet ground (vk_mirror.comp). A
// colored quad of the far ring (U 1) mixed its fog into its colours on the
// CPU in 256ths (d3d_terrain.rs), its specular alpha 255 less the fog's; its
// share goes below zero, to tell its coarse cells from the grid's.
layout(push_constant) uniform Push {
    vec2 scale;
    float depth_test;
} push;
layout(location = 0) in vec3 v_texture;
layout(location = 2) in vec4 v_fog;
layout(location = 3) in float v_depth;
layout(location = 0) out vec4 o_ground;
void main() {
    if (push.depth_test > 0.5 && v_depth > 1.0) discard;
    gl_FragDepth = clamp(v_depth, 0.0, 1.0);
    bool coarse = v_texture.x > 0.5;
    float shows = coarse ? -(v_fog.a * 255.0 + 1.0) / 256.0 : v_fog.a;
    o_ground = vec4(v_fog.rgb, shows);
}
