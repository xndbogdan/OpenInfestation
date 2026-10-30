#version 450
// D3DTBLEND_MODULATEALPHA, then vertex fog: visibility rides in specular
// alpha and fogs RGB after texturing, before destination blending.
layout(push_constant) uniform Push {
    vec2 scale;
    float depth_test;
} push;
layout(set = 0, binding = 0) uniform sampler2D u_texture;
layout(location = 0) in vec3 v_texture;
layout(location = 1) in vec4 v_diffuse;
layout(location = 2) in vec4 v_fog;
layout(location = 3) in float v_depth;
layout(location = 0) out vec4 o_color;
void main() {
    if (push.depth_test > 0.5 && v_depth > 1.0) discard;
    vec4 color = texture(u_texture, v_texture.xy / v_texture.z) * v_diffuse;
    color.rgb = mix(v_fog.rgb, color.rgb, v_fog.a);
    gl_FragDepth = clamp(v_depth, 0.0, 1.0);
    o_color = color;
}
