#version 450
// A supersampled frame onto the screen: each screen pixel the average of
// the frame's pixels under it, each weighted by how much of it the screen
// pixel covers (an exact box filter, for any ratio from one to two).
layout(set = 0, binding = 0) uniform sampler2D u_texture;
layout(location = 0) in vec2 v_texture;
layout(location = 0) out vec4 o_color;

void main() {
    vec2 size = vec2(textureSize(u_texture, 0));
    // The screen pixel's footprint, in frame pixels.
    vec2 scale = abs(vec2(dFdx(v_texture.x), dFdy(v_texture.y))) * size;
    vec2 low = v_texture * size - 0.5 * scale;
    vec2 high = low + scale;
    ivec2 first = ivec2(floor(low));
    ivec2 last = min(ivec2(ceil(high)) - 1, first + 3);
    ivec2 edge = ivec2(size) - 1;
    vec3 sum = vec3(0.0);
    float total = 0.0;
    for (int y = first.y; y <= last.y; y++) {
        for (int x = first.x; x <= last.x; x++) {
            vec2 texel = vec2(x, y);
            vec2 overlap = max(min(high, texel + 1.0) - max(low, texel), vec2(0.0));
            float weight = overlap.x * overlap.y;
            sum += weight * texelFetch(u_texture, clamp(ivec2(x, y), ivec2(0), edge), 0).rgb;
            total += weight;
        }
    }
    o_color = vec4(sum / max(total, 1.0e-6), 1.0);
}
