#version 450
// Depth comes from the fragment, so no TL depth is clipped by the near or
// far plane; the CPU raster does not clip depth either.
layout(push_constant) uniform Push {
    vec2 scale;
    float depth_test;
} push;
layout(location = 1) in vec4 v_diffuse;
layout(location = 3) in float v_depth;
layout(location = 0) out vec4 o_color;
void main() {
    if (push.depth_test > 0.5 && v_depth > 1.0) discard;
    gl_FragDepth = clamp(v_depth, 0.0, 1.0);
    o_color = v_diffuse;
}
