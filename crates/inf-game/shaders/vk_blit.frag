#version 450
layout(push_constant) uniform Push {
    float opaque;
} push;
layout(set = 0, binding = 0) uniform sampler2D u_texture;
layout(location = 0) in vec2 v_texture;
layout(location = 0) out vec4 o_color;
void main() {
    vec4 color = texture(u_texture, v_texture);
    o_color = vec4(color.rgb, max(color.a, push.opaque));
}
