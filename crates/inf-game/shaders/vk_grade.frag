#version 450
// The lighting preset's grade over the traced world and the raster's
// effects on it, before the HUD: the frame's brightness scaled, its colours
// pushed from grey, and its tones bent toward an S-curve, darks darker and
// lights brighter without clipping either end.
layout(set = 0, binding = 0) uniform sampler2D frame_image;
layout(push_constant) uniform Grade {
    float exposure;
    float saturation;
    // The S-curve's share, 0 to 1.
    float curve;
} grade;
layout(location = 0) in vec2 v_texture;
layout(location = 0) out vec4 o_color;

void main() {
    vec4 frame = texture(frame_image, v_texture);
    vec3 color = frame.rgb * grade.exposure;
    float luma = dot(color, vec3(0.2126, 0.7152, 0.0722));
    color = clamp(mix(vec3(luma), color, grade.saturation), 0.0, 1.0);
    color = mix(color, color * color * (3.0 - 2.0 * color), grade.curve);
    o_color = vec4(color, frame.a);
}
