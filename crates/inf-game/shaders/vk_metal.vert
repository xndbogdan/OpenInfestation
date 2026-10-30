#version 450
// vk_primitive.vert for a metal face's fans in Vulkan's raster
// (raster_metal.rs, vk_device/metal.rs): each corner also brings its
// normal, its colour and its V in its sprite, each times its reciprocal
// depth beside that depth, which vk_metal.glsl divides out (the tracer
// blends them across the face in the world; here, w = 1, they blend on the
// screen, as retail's colours do), and the face's material.
layout(push_constant) uniform Push {
    vec2 scale;
    float depth_test;
    // 1 where the modern sky's cube is bound.
    float sky_cube;
    vec2 jitter;
} push;
layout(location = 0) in vec3 a_position;
layout(location = 1) in vec3 a_texture;
layout(location = 2) in vec4 a_diffuse;
layout(location = 3) in vec4 a_fog;
layout(location = 4) in vec4 a_normal;
layout(location = 5) in vec4 a_albedo;
layout(location = 6) in uvec2 a_material;
layout(location = 0) out vec3 v_texture;
layout(location = 1) out vec4 v_diffuse;
layout(location = 2) out vec4 v_fog;
layout(location = 3) out float v_depth;
layout(location = 4) out vec2 v_native;
layout(location = 5) out vec4 v_normal;
layout(location = 6) out vec4 v_albedo;
layout(location = 7) flat out uvec2 v_material;
void main() {
    gl_Position = vec4((a_position.xy + 0.5) * push.scale - 1.0 + push.jitter, 0.0, 1.0);
    v_depth = a_position.z;
    v_texture = a_texture;
    v_diffuse = a_diffuse;
    v_fog = a_fog;
    v_native = a_position.xy + 0.5;
    v_normal = a_normal;
    v_albedo = a_albedo;
    v_material = a_material;
}
