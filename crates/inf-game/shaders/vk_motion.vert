#version 450
// A moving model's solid triangles again, after the raster's world: placed
// as vk_primitive.vert places them, jitter and all, so the depth test finds
// the same surfaces, each corner carrying where its point was in the last
// frame less where it is now, in native pixels.
layout(push_constant) uniform Push {
    vec2 scale;
    float depth_test;
    vec2 jitter;
} push;
layout(location = 0) in vec3 a_position;
layout(location = 1) in vec2 a_motion;
layout(location = 0) out vec2 v_motion;
layout(location = 1) out float v_depth;
void main() {
    gl_Position = vec4((a_position.xy + 0.5) * push.scale - 1.0 + push.jitter, 0.0, 1.0);
    v_depth = a_position.z;
    v_motion = a_motion;
}
