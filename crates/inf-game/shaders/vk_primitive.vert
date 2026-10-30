#version 450
// Direct3D samples pixel centres at integer coordinates and Vulkan at +0.5,
// so the shift keeps coverage on the CPU raster's pixels. `scale` is
// 2 / native size whatever the target's size. Vulkan's framebuffer row 0 is
// NDC y = -1, Direct3D's top row, so readback is already top-down. Colour,
// fog and depth interpolate linearly in screen space (w = 1), as in the CPU
// raster, and UV divides by the interpolated reciprocal depth when the
// submission is perspective-correct. `jitter` moves the world's draws a
// sub-pixel each frame for TAA and the upscalers (in NDC; zero otherwise).
layout(push_constant) uniform Push {
    vec2 scale;
    float depth_test;
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
// The native coordinate of the pixel's centre, for the tracer's fog grid.
layout(location = 4) out vec2 v_native;
void main() {
    gl_Position = vec4((a_position.xy + 0.5) * push.scale - 1.0 + push.jitter, 0.0, 1.0);
    v_depth = a_position.z;
    v_texture = a_texture;
    v_diffuse = a_diffuse;
    v_fog = a_fog;
    v_native = a_position.xy + 0.5;
}
